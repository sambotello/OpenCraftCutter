//! Controlador de las cortadoras Silhouette para OpenCraftCutter.
//!
//! - [`gpgl`]: traducción de los trabajos de Enviar a GPGL (sin E/S, con pruebas).
//! - [`ble`]: conexión por Bluetooth de baja energía.
//! - [`Cutter`]: hilo de trabajo que la interfaz controla con órdenes y del que recibe eventos.

pub mod ble;
pub mod gpgl;

use gpgl::{DetectedTool, Status};
use std::sync::mpsc;
use std::time::Duration;

/// Órdenes de la interfaz.
#[derive(Debug)]
pub enum Command {
    Connect,
    Disconnect,
    /// Consulta el estado y las herramientas.
    Refresh,
    /// Envía los tramos en orden; tras cada tramo marcado con `true` en `pause_after` espera
    /// a [`Command::Resume`].
    Send {
        segments: Vec<Vec<u8>>,
        pause_after: Vec<bool>,
        /// Tramo que termina con la búsqueda de marcas de registro: se envía y se espera la
        /// respuesta de la máquina antes de seguir.
        registration: Option<usize>,
    },
    Resume,
    Cancel,
}

/// Eventos para la interfaz.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Searching,
    Connected {
        name: String,
        firmware: String,
    },
    Disconnected,
    Status(Status),
    Tools(Vec<DetectedTool>),
    /// Bytes enviados y total.
    Progress {
        sent: usize,
        total: usize,
    },
    /// La máquina está leyendo las marcas de registro.
    Registering,
    /// Pausa tras el tramo `n` (empezando en 0).
    Paused(usize),
    Done,
    Cancelled,
    Error(String),
}

/// Controlador: hilo propio con su runtime; la interfaz le habla con [`Cutter::send`] y lee
/// eventos con [`Cutter::poll`].
pub struct Cutter {
    tx: tokio::sync::mpsc::UnboundedSender<Command>,
    rx: mpsc::Receiver<Event>,
}

impl Cutter {
    /// `wake` se llama con cada evento (para repintar la interfaz).
    pub fn spawn(wake: impl Fn() + Send + 'static) -> Cutter {
        let (tx, crx) = tokio::sync::mpsc::unbounded_channel();
        let (etx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("oc-cutter".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_all()
                    .build()
                    .expect("runtime");
                rt.block_on(worker(crx, move |e| {
                    // Registro en stderr de lo que pasa con la máquina (sin el progreso, que es
                    // muy frecuente salvo al empezar y al acabar).
                    match &e {
                        Event::Progress { sent, total } if sent != total && *sent > 1024 => {}
                        _ => eprintln!("[cutter] {e:?}"),
                    }
                    let _ = etx.send(e);
                    wake();
                }));
            })
            .expect("hilo del controlador");
        Cutter { tx, rx }
    }

    pub fn send(&self, c: Command) {
        let _ = self.tx.send(c);
    }

    pub fn poll(&self) -> Vec<Event> {
        self.rx.try_iter().collect()
    }
}

async fn worker(mut rx: tokio::sync::mpsc::UnboundedReceiver<Command>, emit: impl Fn(Event)) {
    let mut conn: Option<ble::Connection> = None;
    let mut tick = tokio::time::interval(Duration::from_secs(3));
    loop {
        tokio::select! {
            cmd = rx.recv() => {
                let Some(cmd) = cmd else { break };
                match cmd {
                    Command::Connect => {
                        if let Some(c) = conn.take() { c.close().await; }
                        emit(Event::Searching);
                        match connect().await {
                            Ok((mut c, fw)) => {
                                emit(Event::Connected { name: c.name.clone(), firmware: fw });
                                refresh(&mut c, &emit).await;
                                conn = Some(c);
                            }
                            Err(e) => emit(Event::Error(e.to_string())),
                        }
                    }
                    Command::Disconnect => {
                        if let Some(c) = conn.take() { c.close().await; }
                        emit(Event::Disconnected);
                    }
                    Command::Refresh => {
                        if let Some(c) = conn.as_mut() { refresh(c, &emit).await; }
                    }
                    Command::Send { segments, pause_after, registration } => match conn.as_mut() {
                        Some(c) => {
                            send_job(c, &mut rx, &segments, &pause_after, registration, &emit).await
                        }
                        None => emit(Event::Error("No hay ninguna máquina conectada.".into())),
                    },
                    Command::Resume | Command::Cancel => {}
                }
            }
            _ = tick.tick() => {
                if let Some(c) = conn.as_mut() {
                    if !c.is_connected().await {
                        conn = None;
                        emit(Event::Disconnected);
                    } else if let Ok(s) = Port::status(c).await {
                        emit(Event::Status(s));
                    }
                }
            }
        }
    }
    if let Some(c) = conn.take() {
        c.close().await;
    }
}

/// Lo que el envío necesita de la máquina: el Bluetooth real o, en las pruebas, una máquina
/// simulada.
trait Port {
    async fn write(&mut self, data: &[u8]) -> Result<(), String>;
    async fn status(&mut self) -> Result<Status, String>;
    async fn tools(&mut self) -> Result<Vec<DetectedTool>, String>;
    /// Escribe y espera la respuesta (descartando respuestas viejas).
    async fn query(&mut self, data: &[u8], timeout: Duration) -> Result<Vec<u8>, String>;
}

impl Port for ble::Connection {
    async fn write(&mut self, data: &[u8]) -> Result<(), String> {
        ble::Connection::write(self, data)
            .await
            .map_err(|e| e.to_string())
    }

    async fn status(&mut self) -> Result<Status, String> {
        ble::Connection::status(self)
            .await
            .map_err(|e| e.to_string())
    }

    async fn tools(&mut self) -> Result<Vec<DetectedTool>, String> {
        ble::Connection::tools(self)
            .await
            .map_err(|e| e.to_string())
    }

    async fn query(&mut self, data: &[u8], timeout: Duration) -> Result<Vec<u8>, String> {
        ble::Connection::query(self, data, timeout)
            .await
            .map_err(|e| e.to_string())
    }
}

/// Lo que puede tardar la máquina en recorrer el papel buscando las marcas.
const REGISTRATION_TIMEOUT: Duration = Duration::from_secs(60);

async fn connect() -> Result<(ble::Connection, String), ble::BleError> {
    let (p, name) = ble::find(8).await?;
    let mut c = ble::Connection::open(p, name).await?;
    let fw = c.firmware().await.unwrap_or_default();
    Ok((c, fw))
}

async fn refresh(c: &mut impl Port, emit: &impl Fn(Event)) {
    if let Ok(s) = c.status().await {
        emit(Event::Status(s));
    }
    if let Ok(t) = c.tools().await {
        emit(Event::Tools(t));
    }
}

/// Envía un trabajo por tramos, informando del progreso y atendiendo pausas y cancelación.
async fn send_job(
    c: &mut impl Port,
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<Command>,
    segments: &[Vec<u8>],
    pause_after: &[bool],
    registration: Option<usize>,
    emit: &impl Fn(Event),
) {
    match c.status().await {
        Ok(Status::Unloaded) => {
            return emit(Event::Error(
                "No hay material cargado: carga el tapete con el botón de la máquina.".into(),
            ));
        }
        Ok(Status::Moving) => {
            return emit(Event::Error("La máquina está ocupada.".into()));
        }
        Err(e) => return emit(Event::Error(e)),
        _ => {}
    }
    let total: usize = segments.iter().map(Vec::len).sum();
    for (i, seg) in segments.iter().enumerate() {
        let text: Vec<String> = seg
            .split(|b| *b == 0x03)
            .filter(|c| !c.is_empty())
            .map(|c| String::from_utf8_lossy(c).into_owned())
            .collect();
        eprintln!(
            "[cutter] segmento {i} ({} bytes): {}",
            seg.len(),
            text.join("  ")
        );
    }
    let mut sent = 0;
    for (i, seg) in segments.iter().enumerate() {
        if registration == Some(i) {
            if let Ok(Command::Cancel) = rx.try_recv() {
                emit(Event::Cancelled);
                return;
            }
            emit(Event::Registering);
            match c.query(seg, REGISTRATION_TIMEOUT).await {
                Ok(r) if gpgl::registration_found(&r) => {}
                Ok(r) => {
                    eprintln!(
                        "[cutter] marcas: respuesta {:?}",
                        String::from_utf8_lossy(&r)
                    );
                    return emit(Event::Error(NO_MARKS.into()));
                }
                Err(e) => {
                    eprintln!("[cutter] marcas: {e}");
                    return emit(Event::Error(NO_MARKS.into()));
                }
            }
            sent += seg.len();
            emit(Event::Progress { sent, total });
            continue;
        }
        for chunk in seg.chunks(1024) {
            if let Ok(Command::Cancel) = rx.try_recv() {
                emit(Event::Cancelled);
                return;
            }
            if let Err(e) = c.write(chunk).await {
                return emit(Event::Error(e));
            }
            sent += chunk.len();
            emit(Event::Progress { sent, total });
        }
        if pause_after.get(i).copied().unwrap_or(false) {
            wait_idle(c).await;
            emit(Event::Paused(i));
            loop {
                match rx.recv().await {
                    Some(Command::Resume) => break,
                    Some(Command::Cancel) | None => {
                        emit(Event::Cancelled);
                        return;
                    }
                    _ => {}
                }
            }
        }
    }
    wait_idle(c).await;
    emit(Event::Done);
}

const NO_MARKS: &str = "La máquina no encontró las marcas de registro. Comprueba que la hoja \
    impresa está bien colocada en el tapete (arriba a la izquierda), que las marcas se ven \
    nítidas y que no hay brillo ni sombras sobre ellas.";

/// Espera a que la máquina termine lo que tiene en cola.
async fn wait_idle(c: &mut impl Port) {
    for _ in 0..600 {
        match c.status().await {
            Ok(Status::Moving) | Err(_) => tokio::time::sleep(Duration::from_millis(700)).await,
            _ => return,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Envío con una máquina simulada: estados, bytes recibidos, pausas, cancelación y errores.

    use super::*;
    use std::sync::{Arc, Mutex};

    /// Máquina simulada: devuelve los estados en orden (repite el último) y guarda lo escrito.
    struct FakeMachine {
        statuses: Vec<Status>,
        written: Arc<Mutex<Vec<u8>>>,
        fail_write_at: Option<usize>,
        writes: usize,
        /// Respuesta a la búsqueda de marcas.
        marks_reply: Result<Vec<u8>, String>,
    }

    impl FakeMachine {
        fn new(statuses: &[Status]) -> (FakeMachine, Arc<Mutex<Vec<u8>>>) {
            let written = Arc::new(Mutex::new(Vec::new()));
            (
                FakeMachine {
                    statuses: statuses.to_vec(),
                    written: written.clone(),
                    fail_write_at: None,
                    writes: 0,
                    marks_reply: Ok(b"    0\x03".to_vec()),
                },
                written,
            )
        }
    }

    impl Port for FakeMachine {
        async fn write(&mut self, data: &[u8]) -> Result<(), String> {
            self.writes += 1;
            if self.fail_write_at == Some(self.writes) {
                return Err("Bluetooth desconectado".into());
            }
            self.written.lock().unwrap().extend_from_slice(data);
            Ok(())
        }

        async fn status(&mut self) -> Result<Status, String> {
            Ok(if self.statuses.len() > 1 {
                self.statuses.remove(0)
            } else {
                self.statuses[0]
            })
        }

        async fn tools(&mut self) -> Result<Vec<DetectedTool>, String> {
            Ok(vec![])
        }

        async fn query(&mut self, data: &[u8], _: Duration) -> Result<Vec<u8>, String> {
            self.written.lock().unwrap().extend_from_slice(data);
            self.marks_reply.clone()
        }
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    fn events() -> (Arc<Mutex<Vec<Event>>>, impl Fn(Event) + Clone) {
        let ev = Arc::new(Mutex::new(Vec::new()));
        let sink = ev.clone();
        (ev, move |e| sink.lock().unwrap().push(e))
    }

    /// Tres tramos; el segundo, más largo que un bloque de envío (1024 bytes).
    fn segments() -> Vec<Vec<u8>> {
        vec![b"TG1\x03".to_vec(), vec![b'D'; 2500], b"FN0\x03".to_vec()]
    }

    #[test]
    fn sends_every_byte_in_order_and_finishes() {
        let (mut m, written) = FakeMachine::new(&[Status::Ready]);
        let (ev, emit) = events();
        let (_tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let segs = segments();
        rt().block_on(send_job(&mut m, &mut rx, &segs, &[false; 3], None, &emit));
        assert_eq!(
            *written.lock().unwrap(),
            segs.concat(),
            "llega todo, en orden"
        );
        let ev = ev.lock().unwrap();
        assert_eq!(ev.last(), Some(&Event::Done));
        let total = segs.iter().map(Vec::len).sum();
        assert!(ev.contains(&Event::Progress { sent: total, total }));
        assert!(
            m.writes >= 5,
            "el tramo largo se parte en bloques de 1024: {}",
            m.writes
        );
    }

    #[test]
    fn refuses_without_material_or_while_moving() {
        for (status, msg) in [(Status::Unloaded, "material"), (Status::Moving, "ocupada")] {
            let (mut m, written) = FakeMachine::new(&[status]);
            let (ev, emit) = events();
            let (_tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            rt().block_on(send_job(
                &mut m,
                &mut rx,
                &segments(),
                &[false; 3],
                None,
                &emit,
            ));
            assert!(written.lock().unwrap().is_empty(), "no se envía nada");
            let ev = ev.lock().unwrap();
            assert!(
                matches!(ev.as_slice(), [Event::Error(e)] if e.contains(msg)),
                "{ev:?}"
            );
        }
    }

    #[test]
    fn waits_while_moving_before_finishing() {
        // Al acabar de enviar, la máquina sigue moviéndose un rato: «Done» llega después.
        let (mut m, _) = FakeMachine::new(&[Status::Ready, Status::Moving, Status::Ready]);
        let (ev, emit) = events();
        let (_tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        rt().block_on(send_job(
            &mut m,
            &mut rx,
            &segments(),
            &[false; 3],
            None,
            &emit,
        ));
        assert_eq!(ev.lock().unwrap().last(), Some(&Event::Done));
    }

    #[test]
    fn pause_then_resume_sends_the_rest() {
        let (mut m, written) = FakeMachine::new(&[Status::Ready]);
        let (ev, emit) = events();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let segs = segments();
        rt().block_on(async {
            let job = send_job(&mut m, &mut rx, &segs, &[false, true, false], None, &emit);
            let resume = async {
                // Espera a la pausa y continúa.
                while !ev.lock().unwrap().contains(&Event::Paused(1)) {
                    tokio::task::yield_now().await;
                }
                tx.send(Command::Resume).unwrap();
            };
            tokio::join!(job, resume);
        });
        assert_eq!(*written.lock().unwrap(), segs.concat());
        let ev = ev.lock().unwrap();
        let paused = ev.iter().position(|e| *e == Event::Paused(1)).unwrap();
        assert!(paused < ev.len() - 1 && ev.last() == Some(&Event::Done));
    }

    #[test]
    fn cancel_during_pause_stops_sending() {
        let (mut m, written) = FakeMachine::new(&[Status::Ready]);
        let (ev, emit) = events();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let segs = segments();
        rt().block_on(async {
            let job = send_job(&mut m, &mut rx, &segs, &[true, false, false], None, &emit);
            let cancel = async {
                while !ev.lock().unwrap().contains(&Event::Paused(0)) {
                    tokio::task::yield_now().await;
                }
                tx.send(Command::Cancel).unwrap();
            };
            tokio::join!(job, cancel);
        });
        assert_eq!(*written.lock().unwrap(), segs[0], "solo el primer tramo");
        assert_eq!(ev.lock().unwrap().last(), Some(&Event::Cancelled));
    }

    #[test]
    fn cancel_before_sending_sends_nothing() {
        let (mut m, written) = FakeMachine::new(&[Status::Ready]);
        let (ev, emit) = events();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        tx.send(Command::Cancel).unwrap();
        rt().block_on(send_job(
            &mut m,
            &mut rx,
            &segments(),
            &[false; 3],
            None,
            &emit,
        ));
        assert!(written.lock().unwrap().is_empty());
        assert_eq!(ev.lock().unwrap().last(), Some(&Event::Cancelled));
    }

    #[test]
    fn write_error_is_reported_and_stops() {
        let (mut m, _) = FakeMachine::new(&[Status::Ready]);
        m.fail_write_at = Some(2);
        let (ev, emit) = events();
        let (_tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        rt().block_on(send_job(
            &mut m,
            &mut rx,
            &segments(),
            &[false; 3],
            None,
            &emit,
        ));
        let ev = ev.lock().unwrap();
        assert!(
            matches!(ev.last(), Some(Event::Error(e)) if e.contains("Bluetooth")),
            "{ev:?}"
        );
        assert!(!ev.contains(&Event::Done));
    }

    #[test]
    fn registration_is_read_before_cutting() {
        let (mut m, written) = FakeMachine::new(&[Status::Ready]);
        let (ev, emit) = events();
        let (_tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let segs = segments();
        rt().block_on(send_job(
            &mut m,
            &mut rx,
            &segs,
            &[false; 3],
            Some(0),
            &emit,
        ));
        assert_eq!(*written.lock().unwrap(), segs.concat());
        let ev = ev.lock().unwrap();
        assert_eq!(ev.first(), Some(&Event::Registering));
        assert_eq!(ev.last(), Some(&Event::Done));
    }

    #[test]
    fn missing_marks_stop_before_cutting() {
        for reply in [
            Ok(b"    1\x03".to_vec()),
            Err("la máquina no respondió".to_string()),
        ] {
            let (mut m, written) = FakeMachine::new(&[Status::Ready]);
            m.marks_reply = reply;
            let (ev, emit) = events();
            let (_tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let segs = segments();
            rt().block_on(send_job(
                &mut m,
                &mut rx,
                &segs,
                &[false; 3],
                Some(0),
                &emit,
            ));
            assert_eq!(
                *written.lock().unwrap(),
                segs[0],
                "solo la búsqueda, ningún trazo"
            );
            let ev = ev.lock().unwrap();
            assert!(
                matches!(ev.last(), Some(Event::Error(e)) if e.contains("marcas de registro")),
                "{ev:?}"
            );
        }
    }
}

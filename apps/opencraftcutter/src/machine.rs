//! Máquina conectada: estado de la conexión, eventos del controlador y envío de trabajos.

use crate::app::OpenCraftCutterApp;
use oc_core::send::{self, Job};
use oc_cutter::gpgl::{self, DetectedTool, Status};
use oc_cutter::{Command, Cutter, Event};

/// Envío en curso.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sending {
    pub sent: usize,
    pub total: usize,
    /// Esperando a que el usuario continúe tras una pausa.
    pub paused: bool,
    pub test: bool,
    /// La máquina está leyendo las marcas de registro.
    pub registering: bool,
}

#[derive(Default)]
pub struct MachineState {
    cutter: Option<Cutter>,
    pub searching: bool,
    /// Nombre y firmware de la máquina conectada.
    pub connected: Option<(String, String)>,
    pub status: Option<Status>,
    pub tools: Vec<DetectedTool>,
    pub sending: Option<Sending>,
    /// Corte de prueba en curso: se apunta al terminar.
    pub pending_test: Option<send::TestAttempt>,
    /// Envío en curso para el historial, y metros de cada herramienta.
    pub pending_history: Option<(oc_core::usage::HistoryEntry, Vec<(send::Tool, f64)>)>,
}

/// Segundos Unix de ahora.
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl MachineState {
    pub fn tool(&self, holder: send::Holder) -> Option<DetectedTool> {
        self.tools.get(holder.number() as usize - 1).copied()
    }

    /// Conectada, con material cargado y sin trabajo en curso.
    pub fn ready(&self) -> bool {
        self.connected.is_some() && self.sending.is_none() && self.status == Some(Status::Ready)
    }

    /// Por qué no se puede enviar ahora (o `None` si se puede).
    pub fn not_ready_reason(&self) -> Option<&'static str> {
        if self.connected.is_none() {
            return Some("Conecta una máquina para enviar el trabajo");
        }
        if self.sending.is_some() {
            return Some("Hay un trabajo en curso");
        }
        match self.status {
            Some(Status::Ready) => None,
            Some(Status::Unloaded) => {
                Some("Carga el tapete con el botón de la máquina antes de enviar")
            }
            Some(Status::Moving) => Some("La máquina está trabajando"),
            _ => Some("Esperando el estado de la máquina"),
        }
    }
}

impl OpenCraftCutterApp {
    fn cutter(&mut self, ctx: &egui::Context) -> &Cutter {
        let ctx = ctx.clone();
        self.machine
            .cutter
            .get_or_insert_with(|| Cutter::spawn(move || ctx.request_repaint()))
    }

    pub fn machine_connect(&mut self, ctx: &egui::Context) {
        self.cutter(ctx).send(Command::Connect);
        self.machine.searching = true;
    }

    pub fn machine_disconnect(&mut self, ctx: &egui::Context) {
        self.cutter(ctx).send(Command::Disconnect);
    }

    pub fn machine_resume(&mut self, ctx: &egui::Context) {
        self.cutter(ctx).send(Command::Resume);
        if let Some(s) = self.machine.sending.as_mut() {
            s.paused = false;
        }
    }

    pub fn machine_cancel(&mut self, ctx: &egui::Context) {
        self.cutter(ctx).send(Command::Cancel);
    }

    /// Lee los eventos del controlador (cada fotograma).
    pub fn machine_poll(&mut self, ctx: &egui::Context) {
        let Some(c) = &self.machine.cutter else {
            return;
        };
        for e in c.poll() {
            // Parte enviada, para el desgaste de un trabajo que no termina.
            let sent_part = self
                .machine
                .sending
                .map(|s| s.sent as f64 / s.total.max(1) as f64)
                .unwrap_or(0.0);
            match &e {
                Event::Done => self.finish_history(oc_core::usage::JobResult::Done, 1.0),
                Event::Cancelled => {
                    self.finish_history(oc_core::usage::JobResult::Cancelled, sent_part)
                }
                Event::Error(msg) => {
                    self.finish_history(oc_core::usage::JobResult::Failed(msg.clone()), sent_part)
                }
                Event::Disconnected => self.finish_history(
                    oc_core::usage::JobResult::Failed("Se perdió la conexión.".into()),
                    sent_part,
                ),
                _ => {}
            }
            let m = &mut self.machine;
            match e {
                Event::Searching => m.searching = true,
                Event::Connected { name, firmware } => {
                    m.searching = false;
                    m.connected = Some((name.clone(), firmware));
                    self.notify(ctx, format!("Conectada: {name}."));
                }
                Event::Disconnected => {
                    m.pending_test = None;
                    m.connected = None;
                    m.status = None;
                    m.tools.clear();
                    m.sending = None;
                }
                Event::Status(s) => m.status = Some(s),
                Event::Tools(t) => m.tools = t,
                Event::Registering => {
                    if let Some(s) = m.sending.as_mut() {
                        s.registering = true;
                    }
                }
                Event::Progress { sent, total } => {
                    if let Some(s) = m.sending.as_mut() {
                        s.sent = sent;
                        s.total = total;
                        s.registering = false;
                    }
                }
                Event::Paused(_) => {
                    if let Some(s) = m.sending.as_mut() {
                        s.paused = true;
                    }
                    self.notify(
                        ctx,
                        "Pausa: cambia la herramienta si hace falta y pulsa Continuar.",
                    );
                }
                Event::Done => {
                    m.sending = None;
                    if let Some(a) = m.pending_test.take() {
                        let n = a.number;
                        self.finish_test_attempt(a);
                        self.notify(
                            ctx,
                            format!(
                                "Prueba {n} terminada. Marca cómo salió en «Intentos de prueba»."
                            ),
                        );
                    } else {
                        self.notify(ctx, "Trabajo terminado.");
                    }
                }
                Event::Cancelled => {
                    m.pending_test = None;
                    m.sending = None;
                    self.notify(ctx, "Trabajo cancelado.");
                }
                Event::Error(msg) => {
                    m.pending_test = None;
                    m.searching = false;
                    m.sending = None;
                    self.notify(ctx, msg);
                }
            }
        }
    }

    /// Prepara la entrada del historial del envío que empieza.
    pub fn start_history(&mut self, jobs: &[Job], test: bool) {
        self.machine.pending_history = Some((
            oc_core::usage::HistoryEntry::new(now_unix(), &self.doc.name, test, jobs),
            oc_core::usage::meters_by_tool(jobs),
        ));
    }

    /// Apunta el envío en curso en el historial y suma a las cuchillas lo cortado
    /// (`part`: fracción enviada).
    pub fn finish_history(&mut self, result: oc_core::usage::JobResult, part: f64) {
        let Some((mut entry, used)) = self.machine.pending_history.take() else {
            return;
        };
        entry.result = Some(result);
        oc_core::usage::push_history(&mut self.prefs.history, entry);
        let used: Vec<(send::Tool, f64)> = used.into_iter().map(|(t, m)| (t, m * part)).collect();
        oc_core::usage::add_wear(&mut self.prefs.blades, &used, now_unix());
    }

    /// Hoja de trabajo para la máquina: tapete y área de corte del documento.
    fn machine_media(&self) -> gpgl::Media {
        let page = &self.doc.page;
        let mat = oc_core::machines::resolve_mat(page.machine, page.mat);
        let (w, h) = if mat.width_mm > 0.0 {
            (mat.width_mm, mat.height_mm)
        } else {
            (page.width_mm, page.height_mm)
        };
        gpgl::Media {
            mat: gpgl::MatCode::from_mat(mat),
            width_mm: w,
            height_mm: h,
            feed: self.prefs.send_options.feed,
            registration: self.registration(),
        }
    }

    /// Marcas de registro de la página (Imprimir y cortar), si están activadas.
    pub fn registration(&self) -> Option<gpgl::Registration> {
        let page = &self.doc.page;
        let r = &page.reg_marks;
        if !r.enabled {
            return None;
        }
        let f = r.cut_area(page.width_mm, page.height_mm);
        Some(gpgl::Registration {
            origin: f.origin(),
            width_mm: f.width(),
            height_mm: f.height(),
            length_mm: r.length_mm,
            thickness_mm: r.thickness_mm,
        })
    }

    /// Por qué no se puede hacer Imprimir y cortar con estas marcas (o `None`).
    pub fn registration_problem(&self) -> Option<&'static str> {
        let r = &self.doc.page.reg_marks;
        (r.enabled && r.inverted).then_some(
            "Las marcas invertidas todavía no se pueden leer: desactiva «Invertir» en Imprimir y cortar, vuelve a imprimir y envía.",
        )
    }

    /// Trabajos del diseño (sin la forma de prueba) o solo de la forma de prueba.
    pub fn jobs_to_send(&mut self, test: bool) -> Vec<Job> {
        let test_ids = send::test_shape_ids(&self.doc);
        let saved = self.doc.shapes.clone();
        self.doc.shapes.retain(|s| test_ids.contains(&s.id) == test);
        let jobs = self.send_jobs();
        self.doc.shapes = saved;
        jobs
    }

    /// Envía el diseño (sin la forma de prueba) o solo la forma de prueba.
    pub fn machine_send(&mut self, ctx: &egui::Context, test: bool) {
        if let Some(why) = self.registration_problem() {
            return self.notify(ctx, why);
        }
        let jobs = self.jobs_to_send(test);
        if jobs.is_empty() {
            return self.notify(ctx, "No hay nada que enviar con estos ajustes.");
        }
        self.start_history(&jobs, test);
        if test {
            let number = self.canvas.send.test_attempts.len() + 1;
            let origin = self.test_shape_origin().unwrap_or_default();
            self.machine.pending_test = send::TestAttempt::from_jobs(number, origin, &jobs);
        }
        let media = self.machine_media();
        let mut segments = gpgl::encode(&media, &jobs);
        // Las pausas van tras el tramo de cada trabajo (antes van los de cabecera).
        let first = gpgl::header_segments(&media);
        let mut pause_after = vec![false; segments.len()];
        for (i, j) in jobs.iter().enumerate() {
            pause_after[i + first] = j.pause_after;
        }
        // Una sola pausa no tiene sentido tras el último trabajo.
        if let Some(p) = pause_after.get_mut(jobs.len() + first - 1) {
            *p = false;
        }
        let total = segments.iter().map(Vec::len).sum();
        self.machine.sending = Some(Sending {
            sent: 0,
            total,
            paused: false,
            test,
            registering: false,
        });
        let segments = std::mem::take(&mut segments);
        self.cutter(ctx).send(Command::Send {
            segments,
            pause_after,
            registration: gpgl::registration_segment(&media),
        });
    }
}

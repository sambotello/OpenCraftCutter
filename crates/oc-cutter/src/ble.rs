//! Conexión con la cortadora por Bluetooth de baja energía. La Cameo expone un servicio que
//! hace de puerto serie: los comandos GPGL se escriben en una característica y las respuestas
//! llegan como indicaciones en otra.

use crate::gpgl::{self, DetectedTool, Status};
use btleplug::api::{
    Central, CharPropFlags, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType,
};
use btleplug::platform::{Manager, Peripheral};
use futures::{Stream, StreamExt};
use std::pin::Pin;
use std::time::Duration;
use uuid::Uuid;

const WRITE_UUID: Uuid = Uuid::from_u128(0x6d92661d_f429_4d67_929b_28e7a9780912);
const READ_UUID: Uuid = Uuid::from_u128(0x8dcf199a_30e7_4bd4_beb6_beb57dca866c);
/// Tamaño de cada escritura (por debajo del MTU habitual de BLE).
const CHUNK: usize = 120;

#[derive(Debug, thiserror::Error)]
pub enum BleError {
    #[error("Bluetooth: {0}")]
    Ble(#[from] btleplug::Error),
    #[error("no hay adaptador Bluetooth")]
    NoAdapter,
    #[error("no se encontró ninguna cortadora Silhouette encendida cerca")]
    NotFound,
    #[error("la cortadora no expone el servicio serie esperado")]
    NoSerial,
    #[error("la cortadora no respondió")]
    Timeout,
}

type Notes = Pin<Box<dyn Stream<Item = btleplug::api::ValueNotification> + Send>>;

pub struct Connection {
    peripheral: Peripheral,
    write: Characteristic,
    notes: Notes,
    pub name: String,
}

/// ¿Es el nombre anunciado el de una cortadora Silhouette?
pub fn is_cutter_name(name: &str) -> bool {
    let u = name.to_uppercase();
    u.contains("CAMEO") || u.contains("PORTRAIT") || u.contains("CURIO")
}

/// Junta las indicaciones hasta ETX o hasta `deadline`; sin nada recibido es `Timeout`.
async fn read_reply<S>(notes: &mut S, deadline: tokio::time::Instant) -> Result<Vec<u8>, BleError>
where
    S: Stream<Item = btleplug::api::ValueNotification> + Unpin,
{
    let mut got = Vec::new();
    while let Ok(Some(n)) = tokio::time::timeout_at(deadline, notes.next()).await {
        got.extend_from_slice(&n.value);
        if got.last() == Some(&gpgl::ETX) {
            return Ok(got);
        }
    }
    if got.is_empty() {
        Err(BleError::Timeout)
    } else {
        Ok(got)
    }
}

/// Versión de firmware sin ETX ni espacios.
fn firmware_text(reply: &[u8]) -> String {
    String::from_utf8_lossy(reply)
        .trim_matches(|c: char| c == '\u{3}' || c.is_whitespace())
        .to_string()
}

/// Busca una cortadora («CAMEO», «PORTRAIT», «CURIO») durante `secs` segundos.
pub async fn find(secs: u64) -> Result<(Peripheral, String), BleError> {
    let manager = Manager::new().await?;
    let adapter = manager
        .adapters()
        .await?
        .into_iter()
        .next()
        .ok_or(BleError::NoAdapter)?;
    adapter.start_scan(ScanFilter::default()).await?;
    let mut found = None;
    for _ in 0..secs * 2 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        for p in adapter.peripherals().await? {
            let name = p.properties().await?.and_then(|pr| pr.local_name);
            if let Some(n) = name.filter(|n| is_cutter_name(n)) {
                found = Some((p, n));
            }
        }
        if found.is_some() {
            break;
        }
    }
    adapter.stop_scan().await?;
    found.ok_or(BleError::NotFound)
}

impl Connection {
    pub async fn open(peripheral: Peripheral, name: String) -> Result<Connection, BleError> {
        if !peripheral.is_connected().await? {
            peripheral.connect().await?;
        }
        peripheral.discover_services().await?;
        let chars = peripheral.characteristics();
        let write = chars
            .iter()
            .find(|c| c.uuid == WRITE_UUID)
            .ok_or(BleError::NoSerial)?
            .clone();
        let read = chars
            .iter()
            .find(|c| c.uuid == READ_UUID)
            .ok_or(BleError::NoSerial)?
            .clone();
        if read
            .properties
            .intersects(CharPropFlags::NOTIFY | CharPropFlags::INDICATE)
        {
            peripheral.subscribe(&read).await?;
        }
        let notes = peripheral.notifications().await?;
        Ok(Connection {
            peripheral,
            write,
            notes,
            name,
        })
    }

    pub async fn is_connected(&self) -> bool {
        self.peripheral.is_connected().await.unwrap_or(false)
    }

    pub async fn write(&self, data: &[u8]) -> Result<(), BleError> {
        for chunk in data.chunks(CHUNK) {
            self.peripheral
                .write(&self.write, chunk, WriteType::WithResponse)
                .await?;
        }
        Ok(())
    }

    /// Envía una consulta y devuelve la respuesta hasta ETX.
    pub async fn query(&mut self, q: &[u8], timeout: Duration) -> Result<Vec<u8>, BleError> {
        // Descarta respuestas pendientes.
        while let Ok(Some(_)) =
            tokio::time::timeout(Duration::from_millis(20), self.notes.next()).await
        {}
        self.write(q).await?;
        read_reply(&mut self.notes, tokio::time::Instant::now() + timeout).await
    }

    pub async fn status(&mut self) -> Result<Status, BleError> {
        let r = self
            .query(gpgl::QUERY_STATUS, Duration::from_secs(3))
            .await?;
        Status::parse(&r).ok_or(BleError::Timeout)
    }

    pub async fn tools(&mut self) -> Result<Vec<DetectedTool>, BleError> {
        let r = self
            .query(gpgl::QUERY_TOOLS, Duration::from_secs(3))
            .await?;
        Ok(gpgl::parse_tools(&r))
    }

    pub async fn firmware(&mut self) -> Result<String, BleError> {
        let r = self
            .query(gpgl::QUERY_FIRMWARE, Duration::from_secs(5))
            .await?;
        Ok(firmware_text(&r))
    }

    pub async fn close(self) {
        let _ = self.peripheral.disconnect().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use btleplug::api::ValueNotification;

    fn note(v: &[u8]) -> ValueNotification {
        ValueNotification {
            uuid: READ_UUID,
            value: v.to_vec(),
        }
    }

    fn soon() -> tokio::time::Instant {
        tokio::time::Instant::now() + Duration::from_millis(200)
    }

    #[test]
    fn recognises_silhouette_names() {
        for n in ["CAMEO 5", "Cameo4-1A2B", "PORTRAIT 3", "curio 2"] {
            assert!(is_cutter_name(n), "{n}");
        }
        for n in ["", "AirPods", "Cricut Maker", "HP LaserJet"] {
            assert!(!is_cutter_name(n), "{n}");
        }
    }

    #[tokio::test]
    async fn reply_joins_chunks_until_etx() {
        let mut s = futures::stream::iter(vec![note(b"0,"), note(b"1\x03"), note(b"sobra")]);
        assert_eq!(read_reply(&mut s, soon()).await.unwrap(), b"0,1\x03");
        // Lo que llegó después de ETX queda para la siguiente consulta.
        assert_eq!(s.next().await.unwrap().value, b"sobra");
    }

    #[tokio::test]
    async fn reply_without_etx_returns_what_arrived() {
        let mut s = futures::stream::iter(vec![note(b"0")]).chain(futures::stream::pending());
        assert_eq!(read_reply(&mut s, soon()).await.unwrap(), b"0");
    }

    #[tokio::test]
    async fn silent_machine_is_a_timeout() {
        let mut s = futures::stream::pending::<ValueNotification>();
        assert!(matches!(
            read_reply(&mut s, soon()).await,
            Err(BleError::Timeout)
        ));
    }

    #[test]
    fn firmware_is_trimmed() {
        assert_eq!(firmware_text(b" V1.04 \x03"), "V1.04");
    }

    /// Prueba con la máquina real: solo consulta (no la mueve). Encender la Cameo y lanzar
    /// `OPENCRAFTCUTTER_BLE=1 cargo test -p oc-cutter -- --ignored real_machine`.
    #[tokio::test]
    #[ignore = "necesita la Cameo encendida"]
    async fn real_machine_answers_queries() {
        if std::env::var_os("OPENCRAFTCUTTER_BLE").is_none() {
            return;
        }
        let (p, name) = find(10).await.expect("cortadora encontrada");
        assert!(is_cutter_name(&name));
        let mut c = Connection::open(p, name).await.expect("conexión");
        let fw = c.firmware().await.expect("firmware");
        assert!(!fw.is_empty());
        c.status().await.expect("estado");
        c.tools().await.expect("herramientas");
        eprintln!("{} · firmware {fw}", c.name);
        c.close().await;
    }
}

use std::path::PathBuf;

/// Datos extraídos del lockfile local del Riot Client.
/// Formato del archivo: `name:pid:port:password:protocol`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockfileInfo {
    pub name: String,
    pub pid: u32,
    pub port: u16,
    pub password: String,
    pub protocol: String,
}

#[derive(Debug, thiserror::Error)]
pub enum LockfileError {
    #[error("no se pudo leer el archivo de lockfile: {0}")]
    Io(#[from] std::io::Error),
    #[error("lockfile malformado: se esperaban 5 campos separados por ':', se encontraron {0}")]
    Malformed(usize),
    #[error("pid inválido en el lockfile: {0}")]
    InvalidPid(std::num::ParseIntError),
    #[error("puerto inválido en el lockfile: {0}")]
    InvalidPort(std::num::ParseIntError),
}

#[cfg(target_os = "windows")]
fn lockfile_path() -> PathBuf {
    let local_app_data =
        std::env::var("LOCALAPPDATA").expect("la variable de entorno LOCALAPPDATA no está definida");
    PathBuf::from(local_app_data).join("Riot Games\\Riot Client\\Config\\lockfile")
}

// En macOS/Linux (o cuando el juego no está instalado) usamos el lockfile
// mockeado del repo, ya que la ruta real de Windows no existe en este entorno.
#[cfg(not(target_os = "windows"))]
fn lockfile_path() -> PathBuf {
    PathBuf::from("mocks/lockfile")
}

/// Lee y parsea el lockfile del Riot Client desde la ruta correspondiente al sistema operativo.
pub fn read_lockfile() -> Result<LockfileInfo, LockfileError> {
    let path = lockfile_path();
    let content = std::fs::read_to_string(path)?;
    parse_lockfile(&content)
}

/// Igual que [`read_lockfile`] pero usando I/O asíncrono de Tokio, para poder
/// invocarse desde el bucle de sondeo (`tokio::time::interval`) sin bloquear
/// el runtime mientras el disco responde.
pub async fn read_lockfile_async() -> Result<LockfileInfo, LockfileError> {
    let path = lockfile_path();
    let content = tokio::fs::read_to_string(path).await?;
    parse_lockfile(&content)
}

/// Parsea el contenido crudo de un lockfile con formato `name:pid:port:password:protocol`.
pub fn parse_lockfile(content: &str) -> Result<LockfileInfo, LockfileError> {
    let parts: Vec<&str> = content.trim().split(':').collect();
    if parts.len() != 5 {
        return Err(LockfileError::Malformed(parts.len()));
    }

    let pid = parts[1].parse::<u32>().map_err(LockfileError::InvalidPid)?;
    let port = parts[2].parse::<u16>().map_err(LockfileError::InvalidPort)?;

    Ok(LockfileInfo {
        name: parts[0].to_string(),
        pid,
        port,
        password: parts[3].to_string(),
        protocol: parts[4].to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mock_lockfile_correctamente() {
        let info = read_lockfile().expect("debería poder leer el lockfile mockeado");

        assert_eq!(info.name, "Riot Client");
        assert_eq!(info.pid, 11240);
        assert_eq!(info.port, 54321);
        assert_eq!(info.password, "mocked_password_12345");
        assert_eq!(info.protocol, "https");
    }

    #[test]
    fn rechaza_contenido_malformado() {
        let result = parse_lockfile("Riot Client:11240:54321:https");
        assert!(matches!(result, Err(LockfileError::Malformed(4))));
    }

    #[tokio::test]
    async fn parses_mock_lockfile_correctamente_de_forma_asincrona() {
        let info = read_lockfile_async()
            .await
            .expect("debería poder leer el lockfile mockeado de forma async");

        assert_eq!(info.name, "Riot Client");
        assert_eq!(info.pid, 11240);
        assert_eq!(info.port, 54321);
        assert_eq!(info.password, "mocked_password_12345");
        assert_eq!(info.protocol, "https");
    }
}

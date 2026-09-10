use std::path::Path;

use base64::{engine::general_purpose::STANDARD, Engine as _};

use crate::models::{MatchHistoryResponse, TokenResponse};

#[derive(Debug, thiserror::Error)]
pub enum RiotClientError {
    #[error("no se pudo leer el archivo mockeado: {0}")]
    Io(#[from] std::io::Error),
    #[error("no se pudo parsear el JSON de la respuesta: {0}")]
    Parse(#[from] serde_json::Error),
}

/// Construye el cliente HTTP para hablar con la API local del Riot Client.
/// Esa API sirve HTTPS con un certificado autofirmado, así que hay que
/// aceptar certificados inválidos o toda petición fallará por TLS.
pub fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("no se pudo construir el cliente HTTP")
}

/// Simula la llamada a `/entitlements/v1/token`, leyendo la respuesta mockeada
/// en vez de hacer una petición real: durante el desarrollo el Riot Client
/// y VALORANT están cerrados, así que un request real fallaría por conexión rechazada.
pub fn fetch_token_mock() -> Result<TokenResponse, RiotClientError> {
    read_json_mock("mocks/token_response.json")
}

/// Simula la llamada a `/match-history/v1/history/{puuid}`, leyendo la
/// respuesta mockeada en vez de hacer una petición real.
pub fn fetch_match_history_mock() -> Result<MatchHistoryResponse, RiotClientError> {
    read_json_mock("mocks/match_history.json")
}

/// Construye la cabecera `Authorization: Basic ...` que exige la API local
/// del Riot Client: usuario fijo `"riot"` y contraseña = la extraída del lockfile.
pub fn build_basic_auth_header(lockfile_password: &str) -> String {
    let credentials = format!("riot:{lockfile_password}");
    let encoded = STANDARD.encode(credentials);
    format!("Basic {encoded}")
}

fn read_json_mock<T: serde::de::DeserializeOwned>(
    path: impl AsRef<Path>,
) -> Result<T, RiotClientError> {
    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construye_cliente_http_aceptando_certificados_autofirmados() {
        let _client = build_http_client();
    }

    #[test]
    fn construye_la_cabecera_basic_auth_con_el_password_del_lockfile() {
        let header = build_basic_auth_header("mocked_password_12345");

        // "riot:mocked_password_12345" en Base64 estándar.
        assert_eq!(header, "Basic cmlvdDptb2NrZWRfcGFzc3dvcmRfMTIzNDU=");
    }
}

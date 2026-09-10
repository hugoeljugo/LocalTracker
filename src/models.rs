use serde::Deserialize;

/// Cherry-pick de `/entitlements/v1/token`: solo los campos que necesitamos
/// para autenticar peticiones posteriores contra la API local y remota de Riot.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    /// El endpoint real llama a este campo simplemente "token".
    #[serde(rename = "token")]
    pub entitlements_token: String,
    /// El PUUID del jugador viaja en el campo "subject".
    pub subject: String,
}

/// Cherry-pick de `/match-history/v1/history/{puuid}`: ignoramos BeginIndex,
/// EndIndex y Total porque no aportan valor a nuestra base de datos.
#[derive(Debug, Clone, Deserialize)]
pub struct MatchHistoryResponse {
    #[serde(rename = "Subject")]
    pub subject: String,
    #[serde(rename = "History")]
    pub history: Vec<MatchHistoryEntry>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct MatchHistoryEntry {
    #[serde(rename = "MatchID")]
    pub match_id: String,
    #[serde(rename = "GameStartTime")]
    pub game_start_time: i64,
    /// Riot devuelve `null` en QueueID para ciertos modos (p.ej. deathmatch/custom),
    /// así que lo tratamos como opcional para no romper el parseo completo.
    #[serde(rename = "QueueID", default)]
    pub queue_id: Option<String>,
}

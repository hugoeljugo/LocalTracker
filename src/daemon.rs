use std::time::Duration;

use crate::lockfile::{self, LockfileError, LockfileInfo};
use crate::riot_client::{self, RiotClientError};

/// Estado conocido del juego en un momento dado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameState {
    Closed,
    Open { puuid: String },
}

impl Default for GameState {
    fn default() -> Self {
        GameState::Closed
    }
}

/// Evento producido al comparar el estado anterior con el nuevo tras un sondeo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameEvent {
    /// El juego pasó de cerrado a abierto.
    GameOpened { puuid: String },
    /// El juego seguía abierto pero el PUUID cambió (cambio de cuenta/usuario).
    AccountSwitched {
        previous_puuid: String,
        new_puuid: String,
    },
    /// El juego pasó de abierto a cerrado.
    GameClosed,
    /// No hubo cambio de estado respecto al sondeo anterior.
    NoChange,
}

/// Ejecuta un ciclo de sondeo: intenta leer el lockfile y, si el juego está
/// abierto, confirma la identidad del jugador. Las funciones de lectura se
/// inyectan como parámetros para poder simular en tests los distintos
/// estados (abierto / cerrado / cambio de cuenta) sin tocar el disco real
/// ni depender de temporizadores.
///
/// Cualquier error de conexión o de lectura se trata como "juego cerrado":
/// es el comportamiento normal cuando el usuario cierra VALORANT de repente,
/// así que no debe propagarse como un fallo, solo actualizar el estado.
pub fn poll_once<L, F>(state: &mut GameState, read_lockfile: L, fetch_puuid: F) -> GameEvent
where
    L: FnOnce() -> Result<LockfileInfo, LockfileError>,
    F: FnOnce() -> Result<String, RiotClientError>,
{
    match read_lockfile() {
        Err(_) => {
            let event = match state {
                GameState::Open { .. } => GameEvent::GameClosed,
                GameState::Closed => GameEvent::NoChange,
            };
            *state = GameState::Closed;
            event
        }
        Ok(_lockfile_info) => match fetch_puuid() {
            // No se pudo confirmar la identidad todavía (p. ej. el Riot Client
            // acaba de arrancar y el token endpoint no responde aún); se
            // reintentará en el siguiente tick sin romper el bucle.
            Err(_) => GameEvent::NoChange,
            Ok(puuid) => {
                let event = match state {
                    GameState::Closed => GameEvent::GameOpened {
                        puuid: puuid.clone(),
                    },
                    GameState::Open { puuid: current } if *current != puuid => {
                        GameEvent::AccountSwitched {
                            previous_puuid: current.clone(),
                            new_puuid: puuid.clone(),
                        }
                    }
                    GameState::Open { .. } => GameEvent::NoChange,
                };
                *state = GameState::Open { puuid };
                event
            }
        },
    }
}

/// Bucle asíncrono de producción: sondea el lockfile real (o el mock en
/// macOS/Linux) cada `interval` y entrega cada evento a `on_event`. No se
/// testea directamente (correría para siempre); la lógica de estado que sí
/// se testea vive en [`poll_once`].
pub async fn run_polling_loop(interval: Duration, mut on_event: impl FnMut(GameEvent)) -> ! {
    let mut state = GameState::default();
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        let event = poll_once(&mut state, lockfile::read_lockfile, || {
            riot_client::fetch_token_mock().map(|token| token.subject)
        });
        on_event(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_lockfile() -> Result<LockfileInfo, LockfileError> {
        Ok(LockfileInfo {
            name: "Riot Client".to_string(),
            pid: 1,
            port: 1,
            password: "pwd".to_string(),
            protocol: "https".to_string(),
        })
    }

    fn closed() -> Result<LockfileInfo, LockfileError> {
        Err(LockfileError::Malformed(0))
    }

    #[test]
    fn detecta_apertura_del_juego_desde_estado_cerrado() {
        let mut state = GameState::Closed;

        let event = poll_once(&mut state, ok_lockfile, || Ok("puuid-a".to_string()));

        assert_eq!(
            event,
            GameEvent::GameOpened {
                puuid: "puuid-a".to_string()
            }
        );
        assert_eq!(state, GameState::Open { puuid: "puuid-a".to_string() });
    }

    #[test]
    fn detecta_cierre_del_juego_desde_estado_abierto() {
        let mut state = GameState::Open {
            puuid: "puuid-a".to_string(),
        };

        let event = poll_once(&mut state, closed, || {
            panic!("no debería llamarse a fetch_puuid si el lockfile falla")
        });

        assert_eq!(event, GameEvent::GameClosed);
        assert_eq!(state, GameState::Closed);
    }

    #[test]
    fn silencia_fallos_de_lectura_sin_pasar_de_cerrado_a_error() {
        let mut state = GameState::Closed;

        let event = poll_once(&mut state, closed, || {
            panic!("no debería llamarse a fetch_puuid si el lockfile falla")
        });

        assert_eq!(event, GameEvent::NoChange);
        assert_eq!(state, GameState::Closed);
    }

    #[test]
    fn detecta_cambio_de_cuenta_multiusuario_por_puuid() {
        let mut state = GameState::Open {
            puuid: "puuid-a".to_string(),
        };

        let event = poll_once(&mut state, ok_lockfile, || Ok("puuid-b".to_string()));

        assert_eq!(
            event,
            GameEvent::AccountSwitched {
                previous_puuid: "puuid-a".to_string(),
                new_puuid: "puuid-b".to_string(),
            }
        );
        assert_eq!(state, GameState::Open { puuid: "puuid-b".to_string() });
    }

    #[test]
    fn no_reporta_cambios_si_el_juego_sigue_abierto_con_la_misma_cuenta() {
        let mut state = GameState::Open {
            puuid: "puuid-a".to_string(),
        };

        let event = poll_once(&mut state, ok_lockfile, || Ok("puuid-a".to_string()));

        assert_eq!(event, GameEvent::NoChange);
        assert_eq!(state, GameState::Open { puuid: "puuid-a".to_string() });
    }

    /// Simula el ciclo de vida completo abierto -> cerrado -> abierto,
    /// leyendo distintos "estados" de mocks encadenados en una sola secuencia
    /// de sondeos, tal como pediría el checklist de validación de la Fase 5.
    #[test]
    fn simula_ciclo_de_vida_completo_abierto_cerrado_abierto() {
        let mut state = GameState::Closed;

        // 1) Abre el juego.
        let e1 = poll_once(&mut state, ok_lockfile, || Ok("puuid-a".to_string()));
        assert_eq!(
            e1,
            GameEvent::GameOpened {
                puuid: "puuid-a".to_string()
            }
        );

        // 2) El usuario cierra VALORANT de repente: el lockfile desaparece.
        let e2 = poll_once(&mut state, closed, || {
            panic!("no debería llamarse a fetch_puuid con el juego cerrado")
        });
        assert_eq!(e2, GameEvent::GameClosed);

        // 3) Vuelve a abrir el juego, potencialmente con otra cuenta.
        let e3 = poll_once(&mut state, ok_lockfile, || Ok("puuid-b".to_string()));
        assert_eq!(
            e3,
            GameEvent::GameOpened {
                puuid: "puuid-b".to_string()
            }
        );
        assert_eq!(state, GameState::Open { puuid: "puuid-b".to_string() });
    }
}

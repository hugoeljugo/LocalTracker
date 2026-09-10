use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};

/// `rusqlite` es síncrono y bloqueante, así que toda interacción con la base
/// de datos se envuelve en `tokio::task::spawn_blocking` para no asfixiar
/// el runtime de Tokio. La conexión se comparte con `Arc<Mutex<..>>` porque
/// `Connection` no es `Sync`.
pub type SharedConnection = Arc<Mutex<Connection>>;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("error de base de datos: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("la tarea bloqueante de la base de datos falló: {0}")]
    Join(#[from] tokio::task::JoinError),
}

#[derive(Debug, Clone)]
pub struct MatchSummary {
    pub match_id: String,
    pub game_start_time: i64,
    pub queue_id: Option<String>,
    /// Rondas jugadas en la partida. Necesario para calcular ADR
    /// (daño promedio por ronda) en las consultas analíticas.
    pub rounds_played: i64,
}

#[derive(Debug, Clone)]
pub struct PlayerMatchStat {
    pub match_id: String,
    pub puuid: String,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub headshots: i64,
    pub bodyshots: i64,
    pub legshots: i64,
    pub damage_dealt: i64,
}

fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS users (
            puuid TEXT PRIMARY KEY
        );

        CREATE TABLE IF NOT EXISTS matches (
            match_id TEXT PRIMARY KEY,
            game_start_time INTEGER NOT NULL,
            queue_id TEXT,
            rounds_played INTEGER NOT NULL DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS player_match_stats (
            match_id TEXT NOT NULL,
            puuid TEXT NOT NULL,
            kills INTEGER NOT NULL DEFAULT 0,
            deaths INTEGER NOT NULL DEFAULT 0,
            assists INTEGER NOT NULL DEFAULT 0,
            headshots INTEGER NOT NULL DEFAULT 0,
            bodyshots INTEGER NOT NULL DEFAULT 0,
            legshots INTEGER NOT NULL DEFAULT 0,
            damage_dealt INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (match_id, puuid),
            FOREIGN KEY (match_id) REFERENCES matches(match_id),
            FOREIGN KEY (puuid) REFERENCES users(puuid)
        );
        ",
    )
}

/// Abre una base de datos en memoria (`:memory:`) con el esquema ya creado.
/// Pensada para tests: no toca el disco.
pub fn open_in_memory() -> Result<SharedConnection, DbError> {
    let conn = Connection::open_in_memory()?;
    init_schema(&conn)?;
    Ok(Arc::new(Mutex::new(conn)))
}

/// Abre (o crea) la base de datos en `path` y aplica el esquema si falta.
/// La apertura del archivo y la creación de tablas se hacen en un hilo
/// bloqueante para no bloquear el runtime de Tokio.
pub async fn init_database(path: impl Into<String> + Send + 'static) -> Result<SharedConnection, DbError> {
    let conn = tokio::task::spawn_blocking(move || -> Result<Connection, DbError> {
        let conn = Connection::open(path.into())?;
        init_schema(&conn)?;
        Ok(conn)
    })
    .await??;
    Ok(Arc::new(Mutex::new(conn)))
}

/// Inserta una partida junto con las estadísticas de todos sus jugadores en
/// una única transacción, para soportar cargas masivas (los ~10 jugadores
/// de una partida de VALORANT) sin golpear el disco fila a fila. Usa
/// `INSERT OR IGNORE` para que reprocesar la misma partida sea idempotente.
pub async fn insert_match_bulk(
    db: SharedConnection,
    match_summary: MatchSummary,
    stats: Vec<PlayerMatchStat>,
) -> Result<(), DbError> {
    tokio::task::spawn_blocking(move || -> Result<(), DbError> {
        let mut conn = db.lock().expect("lock de la conexión envenenado");
        let tx = conn.transaction()?;

        tx.execute(
            "INSERT OR IGNORE INTO matches (match_id, game_start_time, queue_id, rounds_played)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                match_summary.match_id,
                match_summary.game_start_time,
                match_summary.queue_id,
                match_summary.rounds_played
            ],
        )?;

        for stat in &stats {
            tx.execute(
                "INSERT OR IGNORE INTO users (puuid) VALUES (?1)",
                params![stat.puuid],
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO player_match_stats
                    (match_id, puuid, kills, deaths, assists, headshots, bodyshots, legshots, damage_dealt)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    stat.match_id,
                    stat.puuid,
                    stat.kills,
                    stat.deaths,
                    stat.assists,
                    stat.headshots,
                    stat.bodyshots,
                    stat.legshots,
                    stat.damage_dealt,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    })
    .await??;
    Ok(())
}

/// Métricas agregadas de un jugador a través de todas sus partidas
/// registradas: HS%, KDA y ADR (daño promedio por ronda).
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerMetrics {
    pub puuid: String,
    pub matches_played: i64,
    pub headshot_percent: f64,
    pub kda: f64,
    pub adr: f64,
}

/// Calcula HS%, KDA y ADR de un jugador con una única consulta analítica
/// que agrega todas sus filas en `player_match_stats` (unidas con `matches`
/// para obtener las rondas jugadas). Devuelve métricas en cero si el
/// jugador todavía no tiene partidas registradas.
pub async fn player_metrics(db: SharedConnection, puuid: String) -> Result<PlayerMetrics, DbError> {
    tokio::task::spawn_blocking(move || -> Result<PlayerMetrics, DbError> {
        let conn = db.lock().expect("lock de la conexión envenenado");

        let row = conn.query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(pms.kills), 0),
                COALESCE(SUM(pms.deaths), 0),
                COALESCE(SUM(pms.assists), 0),
                COALESCE(SUM(pms.headshots), 0),
                COALESCE(SUM(pms.bodyshots), 0),
                COALESCE(SUM(pms.legshots), 0),
                COALESCE(SUM(pms.damage_dealt), 0),
                COALESCE(SUM(m.rounds_played), 0)
             FROM player_match_stats pms
             JOIN matches m ON m.match_id = pms.match_id
             WHERE pms.puuid = ?1",
            params![puuid],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            },
        )?;

        let (matches_played, kills, deaths, assists, headshots, bodyshots, legshots, damage, rounds) =
            row;

        let total_shots = headshots + bodyshots + legshots;
        let headshot_percent = if total_shots > 0 {
            headshots as f64 / total_shots as f64 * 100.0
        } else {
            0.0
        };
        // Convención estándar cuando no hay muertes: se cuenta como si hubiera 1,
        // para no dividir entre cero y no premiar artificialmente 0 muertes con KDA infinito.
        let kda = (kills + assists) as f64 / deaths.max(1) as f64;
        let adr = if rounds > 0 {
            damage as f64 / rounds as f64
        } else {
            0.0
        };

        Ok(PlayerMetrics {
            puuid,
            matches_played,
            headshot_percent,
            kda,
            adr,
        })
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn crea_las_tablas_en_memoria_sin_errores() {
        let _db = open_in_memory().expect("debería crear el esquema en :memory:");
    }

    #[tokio::test]
    async fn inserta_una_partida_de_prueba_correctamente() {
        let db = open_in_memory().expect("debería crear el esquema en :memory:");

        let match_summary = MatchSummary {
            match_id: "match-test".to_string(),
            game_start_time: 1_700_000_000,
            queue_id: Some("competitive".to_string()),
            rounds_played: 24,
        };
        let stats = vec![PlayerMatchStat {
            match_id: "match-test".to_string(),
            puuid: "puuid-test".to_string(),
            kills: 20,
            deaths: 10,
            assists: 5,
            headshots: 8,
            bodyshots: 15,
            legshots: 2,
            damage_dealt: 3500,
        }];

        insert_match_bulk(db.clone(), match_summary, stats)
            .await
            .expect("debería insertar la partida y las stats en una transacción");

        let conn = db.lock().expect("lock de la conexión envenenado");

        let match_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM matches", [], |row| row.get(0))
            .unwrap();
        assert_eq!(match_count, 1);

        let stats_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM player_match_stats", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(stats_count, 1);

        let user_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
            .unwrap();
        assert_eq!(user_count, 1);
    }

    #[tokio::test]
    async fn reinsertar_la_misma_partida_es_idempotente() {
        let db = open_in_memory().expect("debería crear el esquema en :memory:");

        let match_summary = MatchSummary {
            match_id: "match-dup".to_string(),
            game_start_time: 1,
            queue_id: None,
            rounds_played: 13,
        };
        let stats = vec![PlayerMatchStat {
            match_id: "match-dup".to_string(),
            puuid: "puuid-dup".to_string(),
            kills: 1,
            deaths: 1,
            assists: 1,
            headshots: 1,
            bodyshots: 1,
            legshots: 1,
            damage_dealt: 100,
        }];

        insert_match_bulk(db.clone(), match_summary.clone(), stats.clone())
            .await
            .unwrap();
        insert_match_bulk(db.clone(), match_summary, stats).await.unwrap();

        let conn = db.lock().expect("lock de la conexión envenenado");
        let match_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM matches", [], |row| row.get(0))
            .unwrap();
        assert_eq!(match_count, 1);
    }

    #[tokio::test]
    async fn calcula_hs_kda_y_adr_correctamente_para_un_jugador() {
        let db = open_in_memory().expect("debería crear el esquema en :memory:");

        // Dos partidas del mismo jugador: 24 rondas con 200 de daño total,
        // y 12 rondas con 100 de daño total => 300 daño / 36 rondas = ADR 8.33...
        let stats_match_1 = PlayerMatchStat {
            match_id: "match-1".to_string(),
            puuid: "puuid-metrics".to_string(),
            kills: 20,
            deaths: 10,
            assists: 5,
            headshots: 25,
            bodyshots: 50,
            legshots: 25,
            damage_dealt: 200,
        };
        let stats_match_2 = PlayerMatchStat {
            match_id: "match-2".to_string(),
            puuid: "puuid-metrics".to_string(),
            kills: 10,
            deaths: 10,
            assists: 5,
            headshots: 25,
            bodyshots: 25,
            legshots: 0,
            damage_dealt: 100,
        };

        insert_match_bulk(
            db.clone(),
            MatchSummary {
                match_id: "match-1".to_string(),
                game_start_time: 1,
                queue_id: Some("competitive".to_string()),
                rounds_played: 24,
            },
            vec![stats_match_1],
        )
        .await
        .unwrap();
        insert_match_bulk(
            db.clone(),
            MatchSummary {
                match_id: "match-2".to_string(),
                game_start_time: 2,
                queue_id: Some("competitive".to_string()),
                rounds_played: 12,
            },
            vec![stats_match_2],
        )
        .await
        .unwrap();

        let metrics = player_metrics(db.clone(), "puuid-metrics".to_string())
            .await
            .expect("debería calcular las métricas del jugador");

        assert_eq!(metrics.matches_played, 2);
        // (30 kills + 10 assists) / 20 deaths
        assert!((metrics.kda - 2.0).abs() < 1e-9);
        // 50 headshots / 150 impactos totales * 100
        assert!((metrics.headshot_percent - (50.0 / 150.0 * 100.0)).abs() < 1e-9);
        // 300 daño total / 36 rondas totales
        assert!((metrics.adr - (300.0 / 36.0)).abs() < 1e-9);
    }

    #[tokio::test]
    async fn metricas_de_un_jugador_sin_partidas_son_cero() {
        let db = open_in_memory().expect("debería crear el esquema en :memory:");

        let metrics = player_metrics(db, "puuid-desconocido".to_string())
            .await
            .expect("debería devolver métricas en cero, no un error");

        assert_eq!(metrics.matches_played, 0);
        assert_eq!(metrics.kda, 0.0);
        assert_eq!(metrics.headshot_percent, 0.0);
        assert_eq!(metrics.adr, 0.0);
    }
}

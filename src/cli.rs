use clap::Parser;

/// VALORANT Local Tracker: lee el estado del cliente local de Riot y muestra
/// métricas e historial de partidas. Mientras el juego esté cerrado, todos
/// los flags leen de `mocks/` en lugar de hacer peticiones reales.
#[derive(Parser, Debug)]
#[command(name = "local_tracker", about, version)]
pub struct Cli {
    /// Muestra el estado del lockfile y la cuenta activa (puuid).
    #[arg(long)]
    pub summary: bool,

    /// Muestra el historial de partidas reciente.
    #[arg(long)]
    pub history: bool,

    /// Abre el panel de terminal interactivo (ratatui). Pulsa 'q' para salir.
    #[arg(long)]
    pub tui: bool,
}

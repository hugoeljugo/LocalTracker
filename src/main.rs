use clap::Parser;
use local_tracker::cli::Cli;
use local_tracker::{lockfile, riot_client, tui};

fn main() {
    let cli = Cli::parse();

    if !cli.summary && !cli.history && !cli.tui {
        eprintln!("Usa --summary, --history o --tui. Prueba --help para más detalles.");
        return;
    }

    if cli.summary {
        print_summary();
    }

    if cli.history {
        print_history();
    }

    if cli.tui {
        run_tui();
    }
}

fn print_summary() {
    match lockfile::read_lockfile() {
        Ok(info) => println!("Lockfile OK: puerto={}, protocolo={}", info.port, info.protocol),
        Err(e) => eprintln!("No se pudo leer el lockfile: {e}"),
    }

    match riot_client::fetch_token_mock() {
        Ok(token) => println!("Cuenta activa (puuid): {}", token.subject),
        Err(e) => eprintln!("No se pudo obtener la cuenta activa: {e}"),
    }
}

fn print_history() {
    match riot_client::fetch_match_history_mock() {
        Ok(history) => {
            println!("Historial de partidas ({} encontradas):", history.history.len());
            for entry in &history.history {
                println!(
                    "  - {} | inicio: {} | cola: {}",
                    entry.match_id,
                    entry.game_start_time,
                    entry.queue_id.as_deref().unwrap_or("desconocida")
                );
            }
        }
        Err(e) => eprintln!("No se pudo obtener el historial de partidas: {e}"),
    }
}

fn run_tui() {
    let puuid = riot_client::fetch_token_mock().ok().map(|t| t.subject);
    let matches = riot_client::fetch_match_history_mock()
        .map(|h| h.history)
        .unwrap_or_default();
    let app = tui::App::new(puuid, matches);

    let terminal = ratatui::init();
    let result = tui::run(terminal, app);
    ratatui::restore();

    if let Err(e) = result {
        eprintln!("Error en la TUI: {e}");
    }
}

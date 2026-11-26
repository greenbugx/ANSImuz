use ansimuz::{app, artwork, terminal};

fn run_cli_self_test() -> std::process::ExitCode {
    match ansimuz::ytdlp::run_self_test() {
        Ok(report) => {
            println!("ANSImuz self-test passed");
            println!("yt-dlp version: {}", report.ytdlp_version);
            println!("yt-dlp-ejs version: {}", report.ejs_version);
            println!(
                "js runtime: {} ({})",
                report.js_runtime, report.js_runtime_path
            );
            println!("js solver fixture: {}", report.solver_output);
            println!(
                "yt-dlp executable required: {}",
                if report.ytdlp_binary_needed {
                    "yes"
                } else {
                    "no"
                }
            );
            std::process::ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("ANSImuz self-test failed: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn main() -> std::process::ExitCode {
    ansimuz::paths::configure_python_home();

    if std::env::args().nth(1).as_deref() == Some("--self-test") {
        return run_cli_self_test();
    }

    ansimuz::config::clean_stale_caches();
    artwork::init_picker();

    let mut term = match terminal::setup() {
        Ok(term) => term,
        Err(err) => {
            eprintln!("Terminal setup error: {err}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let result = app::run(&mut term);
    let restore_result = terminal::restore(&mut term);

    if let Err(err) = result {
        eprintln!("Application error: {err}");
        let _ = restore_result;
        return std::process::ExitCode::FAILURE;
    }
    if let Err(err) = restore_result {
        eprintln!("Terminal restore error: {err}");
        return std::process::ExitCode::FAILURE;
    }

    std::process::ExitCode::SUCCESS
}

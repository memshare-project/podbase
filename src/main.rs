use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use podbase::{PODCAST_RSS_MAX_ITEMS, SyncOptions, sync_catalog};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(1)
        }
    }
}

async fn run() -> Result<ExitCode, String> {
    let args = parse_args()?;
    let catalog = sync_catalog(SyncOptions {
        show_id: args.show_id,
        max_items: args.max_items,
    })
    .await?;

    if let Some(parent) = args.out.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|err| format!("mkdir: {err}"))?;
    }
    let json = serde_json::to_string_pretty(&catalog).map_err(|err| format!("json: {err}"))?;
    std::fs::write(&args.out, format!("{json}\n")).map_err(|err| format!("write: {err}"))?;

    let episodes: usize = catalog.shows.iter().map(|show| show.episodes.len()).sum();
    let failed = catalog
        .shows
        .iter()
        .filter(|show| show.error.is_some())
        .count();
    println!(
        "wrote {} shows={} episodes={episodes} failed={failed}",
        args.out.display(),
        catalog.shows.len()
    );
    Ok(if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

struct Args {
    out: PathBuf,
    show_id: Option<String>,
    max_items: usize,
}

fn parse_args() -> Result<Args, String> {
    let mut out = PathBuf::from("data/catalog.json");
    let mut show_id = None;
    let mut max_items = PODCAST_RSS_MAX_ITEMS;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => {
                out = PathBuf::from(args.next().ok_or("--out needs a path")?);
            }
            "--show" => {
                show_id = Some(args.next().ok_or("--show needs an id")?);
            }
            "--max-items" => {
                let raw = args.next().ok_or("--max-items needs a number")?;
                max_items = raw.parse().map_err(|_| format!("bad --max-items {raw}"))?;
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unknown arg {other}")),
        }
    }
    Ok(Args {
        out,
        show_id,
        max_items,
    })
}

fn print_help() {
    println!(
        "\
podbase — 拉取日语播客 RSS，聚合成统一目录

  podbase [--out data/catalog.json] [--show <id>] [--max-items 80]
"
    );
}

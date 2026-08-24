#![forbid(unsafe_code)]

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (opts, print_options) = wcl::cli::parse();
    if print_options {
        println!("concurrency: {}", opts.concurrency);
        return Ok(());
    }

    if opts.tui {
        return wcl::tui::run_tui(opts).await;
    }

    if opts.ignore_robots {
        eprintln!("warning: --ignore-robots is set; robots.txt will not be honored");
    }
    let pages = wcl::crawl::run(&opts).await?;
    wcl::output::render(&pages, &opts)?;
    Ok(())
}

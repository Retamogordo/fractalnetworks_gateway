use anyhow::Result;
use fractal_gateway::Options;
use structopt::StructOpt;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let options = Options::from_args();
    options.run().await?;
    Ok(())
}

use std::env;

use anyhow::*;
use watchman_client::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        bail!("Expected two arguments, got {:?}", &args[1..]);
    }
    let hook_version = args[1]
        .parse::<isize>()
        .context("First arg wasn't a version number")?;
    let last_update_token = &args[2];

    // Use the current working directory as the path.
    let worktree = CanonicalPath::canonicalize(".").context("Couldn't get working directory")?;
    let client = Connector::new()
        .connect()
        .await
        .context("Couldn't connect to watchman daemon")?;

    // Resolve the dir and ensure it is being watched.
    let dir = client
        .resolve_root(worktree)
        .await
        .context("Couldn't resolve working directory")?;

    // Create the clock based on which hook version we're given.
    let clock = Clock::Spec(match hook_version {
        1 => ClockSpec::UnixTimestamp(last_update_token.parse::<i64>()? / 1_000_000_000),
        2 => ClockSpec::StringClock(last_update_token.clone()),
        _ => bail!(
            "Unsupported fsmonitor-watchman hook version: {}",
            hook_version
        ),
    });

    // Query the files changed since last checked.
    let response = client
        .query::<NameOnly>(
            &dir,
            QueryRequestCommon {
                since: Some(clock),
                expression: Some(Expr::Not(Box::new(Expr::DirName(DirNameTerm {
                    path: ".git".into(),
                    depth: Some(RelOp::Equal(0)),
                })))),
                ..Default::default()
            },
        )
        .await?;

    // Hook v2 requires us to update the timestamp
    if hook_version >= 2 {
        let resp_spec = match response.clock {
            Clock::Spec(spec) => spec,
            Clock::ScmAware(fat_data) => fat_data.clock,
        };
        let resp_str = match resp_spec {
            ClockSpec::StringClock(str) => str,
            ClockSpec::UnixTimestamp(stamp) => stamp.to_string(),
        };
        print!("{}\0", resp_str);
    }

    // Print all of the files
    for file in response.files.unwrap() {
        print!("{}\0", file.name.display())
    }

    Ok(())
}

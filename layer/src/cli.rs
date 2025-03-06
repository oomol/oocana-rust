use std::process::{Command, Output};
use tracing::instrument;
use utils::error::{Error, Result};

#[instrument(skip_all)]
pub fn exec(mut cmd: Command) -> Result<Output> {
    let output = cmd
        .output()
        .map_err(|e| Error::from(format!("{cmd:?} failed with: {e}")))?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(Error::from(format!("{cmd:?} fail. output: {output:?}")))
    }
}

#[instrument(skip_all)]
pub fn exec_without_output(mut cmd: Command) -> Result<()> {
    let output = cmd
        .output()
        .map_err(|e| Error::from(format!("{cmd:?} failed with: {e}")))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(Error::from(format!("{cmd:?} fail. output: {output:?}")))
    }
}

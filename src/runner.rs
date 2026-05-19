use std::process::Command;

use anyhow::{bail, Context, Result};

pub trait CommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<()>;

    fn run_allow_failure(&self, program: &str, args: &[&str]) -> Result<()> {
        let _ = (program, args);
        self.run(program, args).or(Ok(()))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RealRunner;

impl CommandRunner for RealRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<()> {
        let output = Command::new(program)
            .args(args)
            .output()
            .with_context(|| format!("failed to spawn {program}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            bail!(
                "command failed: {} {}\nstatus: {}\nstdout: {}\nstderr: {}",
                program,
                args.join(" "),
                output.status,
                stdout.trim(),
                stderr.trim()
            );
        }

        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use std::cell::RefCell;

    use anyhow::Result;

    use super::CommandRunner;

    #[derive(Default)]
    pub struct FakeRunner {
        pub commands: RefCell<Vec<Vec<String>>>,
    }

    impl CommandRunner for FakeRunner {
        fn run(&self, program: &str, args: &[&str]) -> Result<()> {
            let mut command = vec![program.to_string()];
            command.extend(args.iter().map(|arg| (*arg).to_string()));
            self.commands.borrow_mut().push(command);
            Ok(())
        }
    }
}

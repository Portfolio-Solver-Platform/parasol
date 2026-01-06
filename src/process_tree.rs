use nix::sys::signal::{self, Signal};
use nix::unistd;
use std::collections::HashSet;
use std::time::Duration;
use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid solver: {0}")]
    KillSolver(String),
}

/// This function in intended to be called from a new thread from the actual program.
pub fn recursive_force_kill(root_pid: u32) -> Result<()> {
    let system = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing()),
    );

    let mut pids_to_kill = HashSet::new();
    if let Some(target_pgid_raw) = get_process_pgid(root_pid) {
        let target_pgid = target_pgid_raw as u32;

        for (pid, _process) in system.processes() {
            if let Some(proc_pgid) = get_process_pgid(pid.as_u32()) {
                if proc_pgid as u32 == target_pgid {
                    pids_to_kill.insert(*pid);
                }
            }
        }
    }

    // Collect descendants immediately before processes disappear
    let current_targets: Vec<Pid> = pids_to_kill.iter().cloned().collect();
    for target in current_targets {
        collect_descendants(&system, target, &mut pids_to_kill);
    }

    std::thread::sleep(Duration::from_secs(2));

    let system = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing()),
    );

    let current_targets: Vec<Pid> = pids_to_kill.iter().cloned().collect();
    for target in current_targets {
        collect_descendants(&system, target, &mut pids_to_kill);
    }

    for pid in &pids_to_kill {
        let _ = signal::kill(unistd::Pid::from_raw(pid.as_u32() as i32), Signal::SIGKILL);
    }
    if !pids_to_kill.contains(&Pid::from_u32(root_pid)) {
        let _ = signal::kill(unistd::Pid::from_raw(root_pid as i32), Signal::SIGKILL);
    }

    Ok(())
}

pub fn send_signals_to_process_tree(pid: u32, signals: Vec<Signal>) -> Result<()> {
    let system = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing()),
    );
    let mut pids_to_kill = HashSet::new();

    if let Some(target_pgid_raw) = get_process_pgid(pid) {
        let target_pgid = target_pgid_raw as u32;

        for (pid, _process) in system.processes() {
            if let Some(proc_pgid) = get_process_pgid(pid.as_u32()) {
                if proc_pgid as u32 == target_pgid {
                    pids_to_kill.insert(*pid);
                }
            }
        }
    }
    let current_targets: Vec<Pid> = pids_to_kill.iter().cloned().collect();
    for target in current_targets {
        collect_descendants(&system, target, &mut pids_to_kill);
    }

    // Errors are ignored as signals often fail (e.g. process do not exist)
    for pid in &pids_to_kill {
        for signal in signals.iter() {
            let _ = signal::kill(unistd::Pid::from_raw(pid.as_u32() as i32), *signal);
        }
    }
    if !pids_to_kill.contains(&Pid::from_u32(pid)) {
        for signal in signals {
            let _ = signal::kill(unistd::Pid::from_raw(pid as i32), signal);
        }
    }

    Ok(())
}

pub fn collect_descendants(system: &System, parent: Pid, acc: &mut HashSet<Pid>) {
    for (pid, process) in system.processes() {
        if process.parent() == Some(parent) {
            // If we haven't seen this child yet, add it and recurse
            if acc.insert(*pid) {
                collect_descendants(system, *pid, acc);
            }
        }
    }
}

pub fn get_process_pgid(pid: u32) -> Option<i32> {
    let pid_wrapper = nix::unistd::Pid::from_raw(pid as i32);
    match unistd::getpgid(Some(pid_wrapper)) {
        Ok(pgid) => Some(pgid.as_raw()),
        Err(_) => None,
    }
}

pub fn get_process_tree_memory(system: &System, root_pid: u32) -> u64 {
    let root_pid = Pid::from_u32(root_pid);
    let mut total_memory = 0u64;
    let mut pids_to_check = vec![root_pid];

    while let Some(pid) = pids_to_check.pop() {
        if let Some(process) = system.process(pid) {
            total_memory += process.memory();
            for (child_pid, child_process) in system.processes() {
                if child_process.parent() == Some(pid) {
                    pids_to_check.push(*child_pid);
                }
            }
        }
    }

    total_memory
}

use nix::sys::signal::{self, Signal};
use nix::unistd;
use std::collections::HashSet;
use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System};
use tokio::time::{Duration, sleep};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid solver: {0}")]
    KillSolver(String),
}

pub async fn recursive_force_kill(root_pid: u32) -> Result<()> {
    let system = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
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

    let current_targets: Vec<Pid> = pids_to_kill.iter().cloned().collect();
    for target in current_targets {
        collect_descendants(&system, target, &mut pids_to_kill);
    }

    // let duration = Duration::from_secs(2);
    // sleep(duration).await;

    let system = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
    );

    let current_targets: Vec<Pid> = pids_to_kill.iter().cloned().collect();
    for target in current_targets {
        collect_descendants(&system, target, &mut pids_to_kill);
    }

    for pid in &pids_to_kill {
        let _ = signal::kill(unistd::Pid::from_raw(pid.as_u32() as i32), Signal::SIGKILL);
    }

    let _ = signal::kill(unistd::Pid::from_raw(root_pid as i32), Signal::SIGKILL);

    Ok(())
}

fn collect_descendants(system: &System, parent: Pid, acc: &mut HashSet<Pid>) {
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

pub fn get_pids_in_group(target_pgid: u32) -> Vec<u32> {
    let mut system = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
    );

    let mut group_members = Vec::new();

    for (pid, process) in system.processes() {
        if let Some(gid) = process.group_id() {
            if *gid == target_pgid {
                group_members.push(pid.as_u32());
            }
        }
    }

    group_members
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

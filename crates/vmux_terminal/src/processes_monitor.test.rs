use super::*;

fn process_id(byte: u8) -> ProcessId {
    ProcessId([byte; 16])
}

fn process_info(id: ProcessId) -> vmux_service::protocol::ProcessInfo {
    vmux_service::protocol::ProcessInfo {
        id,
        shell: "/bin/sh".to_string(),
        cwd: String::new(),
        cols: 80,
        rows: 24,
        pid: 42,
        created_at_secs: 0,
    }
}

#[test]
fn remove_process_from_cached_list_is_optimistic() {
    let keep = process_id(1);
    let kill = process_id(2);
    let mut list = ServiceProcessList {
        processes: vec![process_info(keep), process_info(kill)],
    };

    remove_processes_from_cached_list(&mut list, [kill]);

    assert_eq!(list.processes.len(), 1);
    assert_eq!(list.processes[0].id, keep);
}

#[test]
fn subtree_usage_sums_whole_tree() {
    let mut procs = HashMap::new();
    procs.insert(
        1,
        ProcSample {
            parent: None,
            cpu: 5.0,
            mem: 100,
        },
    );
    procs.insert(
        2,
        ProcSample {
            parent: Some(1),
            cpu: 10.0,
            mem: 200,
        },
    );
    procs.insert(
        3,
        ProcSample {
            parent: Some(2),
            cpu: 1.0,
            mem: 50,
        },
    );
    procs.insert(
        99,
        ProcSample {
            parent: None,
            cpu: 7.0,
            mem: 999,
        },
    );
    let u = subtree_usage(1, &procs);
    assert_eq!(u.cpu_percent, 16.0);
    assert_eq!(u.mem_bytes, 350);
}

#[test]
fn subtree_usage_missing_root_is_zero() {
    let procs = HashMap::new();
    assert_eq!(subtree_usage(5, &procs), Usage::default());
}

#[test]
fn build_entries_attaches_usage() {
    let id = process_id(1);
    let mut usage = ProcessUsage::default();
    usage.0.insert(
        42,
        Usage {
            cpu_percent: 12.5,
            mem_bytes: 332 * 1024 * 1024,
        },
    );
    let entries = build_process_entries(&[process_info(id)], &usage, &Default::default());
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].pid, 42);
    assert_eq!(entries[0].cpu_percent, 12.5);
    assert_eq!(entries[0].mem_bytes, 332 * 1024 * 1024);
    assert!(!entries[0].attached);
}

#[test]
fn build_entries_defaults_usage_when_missing() {
    let entries = build_process_entries(
        &[process_info(process_id(1))],
        &ProcessUsage::default(),
        &Default::default(),
    );
    assert_eq!(entries[0].cpu_percent, 0.0);
    assert_eq!(entries[0].mem_bytes, 0);
}

use roadsim_application::{FrameSnapshotAdapter, SnapshotErrorCode};
use roadsim_backend_api::{AgentFootprint, AgentState, FrameBatch};
use roadsim_compiled_network::CompiledLaneId;
use roadsim_types::SimulationTick;

fn footprint() -> AgentFootprint {
    AgentFootprint::new(4.5, 1.8).unwrap()
}

fn agent(id: u32, lane: u32, x_m: f64) -> AgentState {
    AgentState::new(id, CompiledLaneId::new(lane), x_m, 2.0, 0.25, footprint()).unwrap()
}

fn frame(tick: u64, agents: Vec<AgentState>) -> FrameBatch {
    FrameBatch::new(SimulationTick::new(tick), agents)
}

#[test]
fn a_backend_frame_becomes_a_gpu_ready_structure_of_arrays() {
    let mut adapter = FrameSnapshotAdapter::new();
    adapter
        .update(&frame(7, vec![agent(3, 1, 10.0), agent(9, 2, 20.0)]))
        .unwrap();

    let snapshot = adapter.snapshot();
    assert_eq!(snapshot.tick(), SimulationTick::new(7));
    assert_eq!(snapshot.len(), 2);
    // Backend agent IDs and compact lane IDs are preserved, not re-indexed,
    // so a picked instance still maps back to the model.
    assert_eq!(snapshot.agent_ids(), &[3, 9]);
    assert_eq!(snapshot.lane_ids(), &[1, 2]);
    assert_eq!(snapshot.x_m(), &[10.0, 20.0]);
    assert_eq!(snapshot.y_m(), &[2.0, 2.0]);
    assert_eq!(snapshot.length_m(), &[4.5, 4.5]);
    assert_eq!(snapshot.width_m(), &[1.8, 1.8]);
    assert!((snapshot.heading_rad()[0] - 0.25).abs() < 1.0e-6);
}

#[test]
fn a_steady_run_reuses_its_buffers_instead_of_allocating_per_frame() {
    let mut adapter = FrameSnapshotAdapter::new();
    let agents: Vec<_> = (0..64)
        .map(|index| agent(index, 0, f64::from(index)))
        .collect();
    adapter.update(&frame(1, agents.clone())).unwrap();
    let capacity = adapter.capacity();
    assert!(capacity >= 64);

    for tick in 2..20 {
        adapter.update(&frame(tick, agents.clone())).unwrap();
    }
    assert_eq!(adapter.capacity(), capacity);
    assert_eq!(adapter.snapshot().len(), 64);

    // A shorter frame shrinks the content but keeps the reserved buffers.
    adapter.update(&frame(20, vec![agent(0, 0, 0.0)])).unwrap();
    assert_eq!(adapter.snapshot().len(), 1);
    assert_eq!(adapter.capacity(), capacity);
}

#[test]
fn an_oversized_frame_is_refused_and_keeps_the_previous_snapshot() {
    let mut adapter = FrameSnapshotAdapter::with_max_agents(2);
    adapter.update(&frame(1, vec![agent(0, 0, 1.0)])).unwrap();

    let error = adapter
        .update(&frame(
            2,
            vec![agent(0, 0, 1.0), agent(1, 0, 2.0), agent(2, 0, 3.0)],
        ))
        .unwrap_err();
    assert_eq!(error.code(), SnapshotErrorCode::AgentLimitExceeded);
    assert_eq!(adapter.snapshot().tick(), SimulationTick::new(1));
    assert_eq!(adapter.snapshot().len(), 1);
}

#[test]
fn an_empty_frame_is_a_valid_snapshot() {
    let mut adapter = FrameSnapshotAdapter::new();
    adapter.update(&frame(4, Vec::new())).unwrap();
    assert!(adapter.snapshot().is_empty());
    assert_eq!(adapter.snapshot().tick(), SimulationTick::new(4));
}

#[test]
fn the_adapter_never_reserves_more_than_the_declared_bound() {
    let adapter = FrameSnapshotAdapter::with_max_agents(usize::MAX);
    assert_eq!(
        adapter.max_agents(),
        roadsim_application::MAX_SNAPSHOT_AGENTS
    );
}

use aether_protocol::AetherHeader;
use aether_transport::{
    LinkManager, SimTransportEngine, StreamConfig, StreamManager, TransportEngine,
};
use aether_types::{Packet, Sequence, StreamId, Timestamp};
use fault_injection::FaultInjectionConfig;
use fpga_emulator::{FpgaEmulator, RadioTimingConfig};
use memory_manager::{MemoryBackend, MemoryKind, SimMemory};

/// Integration Test 1: FPGA emulator → transport → memory
#[test]
fn test1_single_stream_fpga_transport_memory() {
    let timing =
        RadioTimingConfig::from_yaml_str(include_str!("../../../configs/radio_timing.yaml"))
            .unwrap();
    let mut fpga = FpgaEmulator::new(timing, StreamId(1));
    let deadline = include_str!("../../../configs/transport_deadline.yaml");
    let mut transport = SimTransportEngine::from_yaml(deadline).unwrap();
    transport.link_up().unwrap();
    transport
        .create_stream(StreamConfig {
            stream_id: StreamId(1),
            carrier: 0,
            antenna: 0,
            qos: 0,
            deadline_ns: 10_000,
        })
        .unwrap();
    transport.start_stream(StreamId(1)).unwrap();

    let packet = fpga.emit_symbol();
    transport.now_ns = packet.timestamp.0 + 100;
    let header = AetherHeader::new(
        packet.stream_id,
        packet.timestamp,
        packet.sequence,
        packet.payload.len() as u32,
    );
    assert!(header.validate().is_ok());

    transport.ingest(packet.clone()).unwrap();
    let received = transport.receive().unwrap().unwrap();

    let mut mem = SimMemory::new();
    let buf = mem
        .allocate(received.payload.len(), MemoryKind::Host)
        .unwrap();
    mem.write(buf, 0, &received.payload).unwrap();
    assert_eq!(
        mem.read(buf, 0, received.payload.len()).unwrap(),
        packet.payload
    );
}

#[test]
fn test2_100g_serialize_model() {
    let yaml = include_str!("../../../configs/ethernet_model.yaml");
    let cfg = ethernet_model::EthernetModelConfig::from_yaml_str(yaml).unwrap();
    assert!((cfg.bandwidth_gbps - 100.0).abs() < f64::EPSILON);
    let delay = cfg.serialize_delay_ns(1_250_000_000);
    assert_eq!(delay, 100_000_000);
}

/// Integration Test 3: 8 antenna × 4 carrier style multi-stream (32 streams capped sample)
#[test]
fn test3_multi_stream_antennas_carriers() {
    let timing =
        RadioTimingConfig::from_yaml_str(include_str!("../../../configs/radio_timing.yaml"))
            .unwrap();
    assert_eq!(timing.max_antennas, 8);
    assert_eq!(timing.max_carriers, 4);

    let deadline = include_str!("../../../configs/transport_deadline.yaml");
    let mut transport = SimTransportEngine::from_yaml(deadline).unwrap();
    transport.link_up().unwrap();

    let mut stream_id = 1u32;
    let mut fpgas = Vec::new();
    for antenna in 0..timing.max_antennas {
        for carrier in 0..timing.max_carriers {
            let id = StreamId(stream_id);
            transport
                .create_stream(StreamConfig {
                    stream_id: id,
                    carrier,
                    antenna,
                    qos: 0,
                    deadline_ns: 10_000,
                })
                .unwrap();
            transport.start_stream(id).unwrap();
            fpgas.push(FpgaEmulator::new(timing.clone(), id));
            stream_id += 1;
        }
    }
    assert_eq!(fpgas.len(), 32);

    for fpga in &mut fpgas {
        let pkt = fpga.emit_symbol();
        transport.now_ns = pkt.timestamp.0 + 50;
        transport.ingest(pkt).unwrap();
    }

    let mut count = 0;
    while transport.receive().unwrap().is_some() {
        count += 1;
    }
    assert_eq!(count, 32);
}

/// Integration Test 4: packet loss + resync recovery + late packet detection
#[test]
fn test4_fault_recovery_loss_and_late() {
    let fault = FaultInjectionConfig::from_yaml_str(
        r#"
version: "1.0.0"
id: fault-test
enabled: true
loss_rate: 0.001
extra_latency_us: 5.0
burst_length: 2
kernel_delay_us: 100.0
"#,
    )
    .unwrap();

    let mut transport =
        SimTransportEngine::from_yaml(include_str!("../../../configs/transport_deadline.yaml"))
            .unwrap();
    transport.link_up().unwrap();
    let id = StreamId(9);
    transport
        .create_stream(StreamConfig {
            stream_id: id,
            carrier: 0,
            antenna: 0,
            qos: 0,
            deadline_ns: 10_000,
        })
        .unwrap();
    transport.start_stream(id).unwrap();

    // Emit seq 0..=4; drop burst starting at 1 (length 2) → drop 1,2
    for seq in 0u64..5 {
        let pkt = Packet::new(id, Sequence(seq), Timestamp(seq * 100), vec![seq as u8]);
        if fault.should_drop_burst(seq, 1) {
            continue;
        }
        transport.now_ns = pkt.timestamp.0 + 100;
        match transport.ingest(pkt) {
            Ok(()) => {}
            Err(aether_transport::TransportError::SequenceGap { got, .. }) => {
                transport.recover_sequence(id, got);
                let retry = Packet::new(id, Sequence(got), Timestamp(got * 100), vec![got as u8]);
                transport.now_ns = retry.timestamp.0 + 100;
                transport.ingest(retry).unwrap();
            }
            Err(e) => panic!("unexpected: {e}"),
        }
    }
    assert!(transport.sequence_gaps >= 1);

    // Late packet after recovery path
    let late = Packet::new(id, Sequence(5), Timestamp(0), vec![5]);
    transport.now_ns = 50_000;
    assert!(matches!(
        transport.ingest(late),
        Err(aether_transport::TransportError::LatePacket { .. })
    ));
    assert_eq!(transport.late_packets, 1);

    // GPU slowdown budget from fault config is available for callers
    assert_eq!(fault.kernel_delay_ns(), 100_000);
    assert_eq!(fault.extra_latency_ns(), 5_000);
}

/// CX5 DMA + GPU ring path
#[test]
fn test5_cx5_dma_and_gpu_ring() {
    use cx5_emulator::{Cx5Nic, PacketIO};
    use gpu_runtime::{BufferState, GpuRingBuffer};

    let mut nic = Cx5Nic::from_yaml(include_str!("../../../configs/nic_dma.yaml")).unwrap();
    let mut ring =
        GpuRingBuffer::from_yaml(include_str!("../../../configs/gpu_ring.yaml")).unwrap();

    nic.advance_time(0);
    nic.submit_rx(Packet::new(
        StreamId(1),
        Sequence(0),
        Timestamp(0),
        vec![1, 2, 3, 4],
    ))
    .unwrap();
    assert!(nic.rx_burst(8).is_empty());
    nic.advance_time(nic.dma_latency_ns());
    let pkts = nic.rx_burst(8);
    assert_eq!(pkts.len(), 1);

    let latency = ring
        .process_packet(&pkts[0].payload, nic.dma_latency_ns())
        .unwrap();
    assert_eq!(latency, ring.kernel_delay_ns());
    assert_eq!(ring.count_state(BufferState::Free), ring.slot_count());
}

/// Integration Test 6: DPDK mock PacketIO + pooled H2D memory
#[test]
fn test6_dpdk_mock_and_pooled_memory() {
    use cx5_emulator::{DpdkPacketIO, PacketIO};
    use memory_manager::{MemoryBackend, MemoryKind, PooledMemory};

    let mut dpdk = DpdkPacketIO::open_yaml(include_str!("../../../configs/backends/dpdk.yaml"))
        .expect("mock dpdk");
    let pkt = Packet::new(StreamId(1), Sequence(0), Timestamp(0), vec![3; 64]);
    dpdk.inject_rx([pkt]).unwrap();
    let burst = dpdk.rx_burst(32);
    assert_eq!(burst.len(), 1);
    assert_eq!(dpdk.tx_burst(burst.clone()), 1);

    let mut mem =
        PooledMemory::from_yaml(include_str!("../../../configs/memory_pool.yaml")).unwrap();
    let host = mem
        .allocate(burst[0].payload.len(), MemoryKind::Host)
        .unwrap();
    mem.write(host, 0, &burst[0].payload).unwrap();
    let (_gpu, ns) = mem
        .copy(host, 0, burst[0].payload.len(), MemoryKind::Gpu)
        .unwrap();
    assert!(ns >= 200);
}

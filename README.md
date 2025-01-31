# Distributed System with Ring Topology

A distributed system implementation featuring a ring topology with resource management and deadlock detection capabilities. The system uses both REST API and RPC for inter-node communication.

## Features

- Ring topology with self-healing capabilities
- Distributed resource management
- Chandy-Misra-Haas deadlock detection algorithm
- REST API for external control
- RPC for inter-node communication

## Architecture

### Node Structure
Each node maintains connections to:
- Next node
- Next-next node (for topology repair)
- Previous node

### Communication
- REST API on port N+1 for external control
- RPC on port N for inter-node communication
- Binary protocol using tarpc for RPC

## Building and Running

### Prerequisites
- Rust toolchain
- Cargo

### Building
```bash
cargo build --release
```

### Running a Node
```bash
./target/release/distributed_system <ip> <port> <resource_name>
```

Example:
```bash
./target/release/distributed_system 127.0.0.1 2010 A
./target/release/distributed_system 127.0.0.1 2020 B
./target/release/distributed_system 127.0.0.1 2030 C
```

### Control Script
A bash script (`control.sh`) is provided to interact with the system:

```bash
chmod +x control.sh
./control.sh
```

Available commands:
- `g <idx>` - Get node health
- `s <idx>` - Get node status
- `j <from_idx> <to_idx>` - Join nodes
- `l <idx>` - Node leaves
- `k <idx>` - Kill node
- `r <idx>` - Revive node
- `acq <idx> <resource>` - Acquire resource
- `rel <idx> <resource>` - Release resource
- `det <idx>` - Start deadlock detection
- `wait <idx> <target_idx>` - Wait for message
- `active <idx>` - Set node active
- `passive <idx>` - Set passive
- `delay <idx> <ms>` - Set delay

## Example Usage

### Creating a Ring Topology
```bash
# Start 3 nodes in separate terminals
./target/release/distributed_system 127.0.0.1 2010 A
./target/release/distributed_system 127.0.0.1 2020 B
./target/release/distributed_system 127.0.0.1 2030 C

# In control script:
j 1 0  # Join node 0 to 1
j 2 1  # Join node 1 to 2
```

## Implementation Details

### Resource Management
- Resources are uniquely identified by strings
- Each resource has one owner
- Resource requests are queued when busy
- Resource state includes current user and request queue

### Deadlock Detection
- Implements Chandy-Misra-Haas algorithm
- Uses probe messages for cycle detection
- Supports active/passive state transitions
- Detection can be initiated from any node

### Fault Tolerance
- Self-healing ring topology
- Node failure detection
- Topology repair mechanism
- Resource state recovery

## License

This project is licensed under the MIT License - see the LICENSE.md file for details.

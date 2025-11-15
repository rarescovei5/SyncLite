## 🧭 **Overview**

**SyncLite** is a **peer-to-peer (P2P) file synchronization CLI** written in **Rust**.  
It allows computers on the same LAN to **sync folders directly over TCP**, with file integrity checks, local state tracking, and conflict resolution — leveraging both the Rust standard library and select external crates for enhanced functionality.

It's like **"Git for live folders"** — not for version history, but for real-time state synchronization with tombstone tracking for deleted files.

## ⚙️ **Core Architecture**

SyncLite is organized into clear, modular components:

### 📁 **Project Structure**
```
src/
├── models/           # Domain models (SyncState, PeersConfig, FileEntry)
├── storage/          # .synclite directory operations and JSON I/O
├── app/              # Global app configuration and directory registry
├── cli/              # Command-line parsing and argument handling
├── sync/             # Synchronization logic and state management
└── utils/            # Generic utilities (output, confirmation, error handling)
```

## 💡 **Command-Line Usage**

### 1. Serve Mode

Host a directory on the network.

```bash
synclite serve ./project
```

**What happens:**
* **Initialization**: Checks if directory is already initialized, creates `.synclite/` if needed
* **Registry**: Adds directory to global app registry to prevent conflicts
* **State scanning**: Recursively scans directory, calculates SHA-256 hashes for all files
* **Tombstone tracking**: Compares current state with stored state, marks missing files as deleted
* **Network setup**: Starts TCP listener on specified port (default: 8080)
* **Peer management**: Initializes peer configuration and leader status

### 2. Connect Mode

Connect to a peer and sync your local directory with theirs.

```bash
synclite connect ./my-copy
```

**What happens:**
* **Directory validation**: Ensures target directory exists and is properly initialized
* **State comparison**: Compares local file state with remote peer's state
* **Conflict detection**: Identifies new, modified, and deleted files on both sides
* **Sync resolution**: Transfers only changed files, applying tombstone markers for deletions
* **State persistence**: Updates local `.synclite/state.json` with new file hashes and sync timestamp

## 📁 **Storage Structure**

Each synced directory contains a hidden `.synclite/` folder with the following structure:

```
.synclite/
├── state.json      # File hashes, timestamps, and tombstone markers
└── peers.json      # Peer configuration and leader information
```

### **state.json** - File State Tracking
```json
{
  "files": {
    "src/main.rs": {
      "hash": "a1b2c3d4e5f6...",
      "is_deleted": false
    },
    "deleted_file.txt": {
      "hash": null,
      "is_deleted": true
    }
  },
  "last_sync": "2025-11-14T23:07:42Z"
}
```

### **peers.json** - Network Configuration
```json
{
  "leader": "192.168.1.42",
  "peers": ["192.168.1.12", "192.168.1.15"]
}
```

**Key Features:**
* **Tombstone tracking**: Deleted files are marked with `is_deleted: true` rather than removed
* **SHA-256 integrity**: Each file has a cryptographic hash for corruption detection
* **Peer failover**: Automatic leader election if the current leader disconnects
* **Conflict prevention**: Global registry prevents overlapping sync directories

## 🔄 **Sync Process (Current Implementation)**

### **Initialization Phase**
1. **Directory Setup**: Creates `.synclite/` directory if it doesn't exist
2. **Registry Check**: Validates no conflicting sync directories exist in parent/child paths
3. **State Creation**: Initializes `state.json` and `peers.json` with default values
4. **File Scanning**: Recursively walks directory tree, calculating SHA-256 for each file

### **State Reconciliation**
1. **Load Stored State**: Reads previous sync state from `.synclite/state.json`
2. **Scan Current Files**: Generates current directory state with fresh file hashes
3. **Tombstone Creation**: Files present in stored state but missing from current scan are marked as deleted:
   ```rust
   // Files that existed before but are now missing get tombstone markers
   if !current_files.contains_key(&stored_file_path) {
       current_files.insert(stored_file_path, FileEntry::new_deleted());
   }
   ```
4. **State Persistence**: Writes updated state back to `.synclite/state.json`

### **File Entry Structure**
Each file is tracked with detailed metadata:
```rust
pub struct FileEntry {
    pub hash: Option<String>,  // SHA-256 hash, None if deleted
    pub is_deleted: bool,      // Tombstone marker
}
```

**File States:**
* **Active**: `hash: Some("abc123..."), is_deleted: false`
* **Deleted**: `hash: None, is_deleted: true`
* **Modified**: Hash changes between sync cycles

## 🏗️ **Development Status**

### **Currently Implemented**
✅ **Core Architecture**: Modular structure with clear separation of concerns  
✅ **File State Tracking**: SHA-256 hashing with tombstone deletion markers  
✅ **Directory Management**: Initialization, validation, and conflict prevention  
✅ **Global Registry**: App-level tracking of sync directories  
✅ **CLI Interface**: Command parsing with serve/connect modes  
✅ **JSON Persistence**: State and peer configuration storage  

### **In Development**
🚧 **Network Layer**: TCP server/client implementation  
🚧 **Sync Protocol**: File transfer and state exchange  
🚧 **Peer Management**: Leader election and failover  
🚧 **Conflict Resolution**: File conflict detection and resolution  

### **Future Features**
📋 **.syncignore System**: Git-like file exclusion patterns  
📋 **Real-time Sync**: File system watching for instant updates  
📋 **Encryption**: TLS/SSL for secure transfers  
📋 **Peer Discovery**: UDP broadcast for automatic peer detection

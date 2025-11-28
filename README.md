# 💫 SyncLite

## 🧭 **Overview**

**SyncLite** is a **real-time peer-to-peer (P2P) file synchronization CLI** written in **Rust**.  
It enables computers on the same network to **sync folders directly over TCP** with:

- ⚡ **Real-time file watching** - Changes propagate instantly across all peers
- 🔒 **SHA-256 integrity checks** - Ensures file consistency across the network
- 🗑️ **Tombstone tracking** - Deleted files are properly synchronized
- 📁 **Directory operations** - Move, create, and delete directories seamlessly
- 🌐 **Multi-peer support** - One leader can sync with multiple connected peers

It's like **"Dropbox for your LAN"** — lightweight, decentralized, and built for speed.

## 🚀 **Quick Start**

### **Local Network Example**

**Computer 1 (Leader):**

```bash
synclite serve ./shared-folder 8080
# Server binds to 0.0.0.0:8080 (accessible on LAN)
```

**Computer 2 (Peer):**

```bash
synclite connect ./synced-folder 8080
# Auto-detects leader's local IP and connects
```

**Computer 3 (Another Peer):**

```bash
synclite connect ./my-copy 8080
# Connects to same leader
# Receives updates from both the leader and Computer 2
```

Now any changes on any computer instantly propagate to all others! 🎉

## 💡 **Command-Line Usage**

### 1. Serve Mode (Leader)

Start a server that watches a directory and allows peers to connect.

```bash
synclite serve ./project [port]
```

### 2. Connect Mode (Peer)

Connect to a leader and keep your directory in sync.

```bash
synclite connect ./my-copy [port]
```

## 📁 **Storage Structure**

Each synced directory contains a hidden `.synclite/` folder:

```
.synclite/
├── state.json      # File hashes, timestamps, and tombstone markers
└── peers.json      # Peer configuration and leader information
```

### **state.json** - File State Tracking

```json
{
  "src/main.rs": {
    "hash": "a1b2c3d4e5f6a7b8c9d0...",
    "is_deleted": false,
    "last_modified": "2025-11-28T14:23:15.123456Z"
  },
  "deleted_file.txt": {
    "hash": null,
    "is_deleted": true,
    "last_modified": "2025-11-28T13:10:42.987654Z"
  }
}
```

### **peers.json** - Network Configuration

```json
{
  "leader": "peer_a1b2c3d4",
  "peers": ["peer_x9y8z7w6", "peer_m5n4o3p2"]
}
```

## 🔄 **Sync Process**

### **Initial Sync (When Peer Connects)**

1. **Peer sends state** → `InitialSyncPush { sync_state }`
2. **Server compares states** → Calls `determine_winning_files()` (Last Write Wins)
3. **Server responds** → `InitialSyncPushResponse { files_to_update, files_to_delete, files_to_send_back }`
4. **Both sides apply changes** → Write winning files, delete losing files
5. **Peer sends requested files** → `FileUpdatePush { files_to_write, paths_to_delete }`
6. **Server broadcasts** → Forwards received files to all other connected peers

### **Real-time Sync (File Watcher)**

Both server and peers watch their directories using the `notify` crate:

1. **Event Detection**: File system events (create/modify/delete) are captured
2. **Event Debouncing**: 150ms window to absorb rapid-fire changes (e.g., atomic saves)
3. **Event Grouping**: Multiple events for the same file are consolidated
4. **State Update**: Local `SyncConfig` is updated with new hashes/tombstones
5. **Network Broadcast**: Changes are sent to all peers via `FileUpdatePush`
6. **Peer Application**: Remote peers receive updates and apply them to their filesystem

## 🏗️ **Development Status**

### **✅ Fully Implemented**

✅ **Real-time File Watching**: `notify` crate with event debouncing and grouping  
✅ **TCP Network Layer**: Server/client with `PeerConnectionManager` for multi-peer support  
✅ **Message Protocol**: `InitialSyncPush`, `InitialSyncPushResponse`, `FileUpdatePush`  
✅ **Conflict Resolution**: Last Write Wins (LWW) based on timestamps  
✅ **Directory Operations**: Recursive scanning on create, batch deletion on remove  
✅ **Unified Sync Methods**: `sync_write_file()`, `sync_batch_delete_files()` keep state + filesystem in sync  
✅ **SHA-256 Integrity**: File hashing for change detection  
✅ **Tombstone Tracking**: Deleted files are marked, not removed from state  
✅ **Multi-peer Broadcasting**: Server forwards updates to all connected peers  
✅ **Sandboxed Operations**: Filesystem safety checks prevent escaping workspace
✅ **Peer Discovery**: mDNS/UDP broadcast for automatic peer detection on LAN  

### **🚧 In Progress**

🚧 **Timestamp Sync**: Ensuring `last_modified` is preserved across network transfers  
🚧 **Error Recovery**: Graceful handling of partial sync failures

### **📋 Future Features**

📋 **.syncignore System**: Git-like file exclusion patterns  
📋 **Encryption**: TLS/SSL for secure transfers over internet  
📋 **Compression**: File compression for large transfers  
📋 **Bandwidth Throttling**: Limit sync speed to prevent network saturation

---

## 📊 **Log Output**

SyncLite uses color-coded emoji logging for easy monitoring:

- 🟢 **Green** (`✨ Creating`, `📁 Directory`) - New files/directories
- 🟡 **Yellow** (`✏️ Modifying`) - File modifications
- 🔴 **Red** (`🗑️ Deleting`) - Deletions
- 🔵 **Blue** (`📡 Broadcasting`, `📥 Received`, `📤 Sending`) - Network operations

## 📄 **License**

MIT License - See `LICENSE` file for details

## 🤝 **Contributing**

This is a learning project focused on:

- Async Rust with Tokio
- P2P networking patterns
- File system watching and state management
- Building CLI tools

Contributions, issues, and feedback welcome!

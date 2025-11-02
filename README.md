## 🧭 **Overview**

**SyncLite** is a **peer-to-peer (P2P) file synchronization CLI** built entirely in **pure Rust (no external crates)**.  
It allows computers on the same LAN to **sync folders directly over TCP**, with file integrity checks, local state tracking, and `.syncignore` support — all using only the Rust standard library.

It’s like **“Git for live folders”** — not for version history, but for real-time state sync.

## ⚙️ **Core Architecture**

| Component                | Description                                                                                 |
| ------------------------- | ------------------------------------------------------------------------------------------- |
| **Network layer**         | Built on `std::net::TcpListener`, `TcpStream`, and raw sockets for direct LAN communication |
| **File I/O layer**        | Uses `std::fs` for recursive directory walking, reading, writing, and hashing               |
| **Sync protocol**         | Simple framing: filename length → filename → file size → file bytes                         |
| **Concurrency**           | Multithreaded transfers with `std::thread` and `mpsc` for parallel file sending/receiving   |
| **Integrity checks**      | Simple checksum (byte-sum or hash) per file to detect corruption or modification            |
| **State tracking**        | Each directory has a `.synclite/state.json` with file checksums and timestamps              |
| **Ignore system**         | `.syncignore` file to exclude unwanted files and directories                                |
| **Conflict handling**     | “Newer file wins” or dual preservation (`file.conflict`) if both sides changed              |
| **Peer management**       | Multi-peer support with automatic leader (server) reassignment if host disconnects          |
| **Workspace identity**    | Each folder has a unique `workspace_id` stored in `.synclite/workspace.json`                |

## 💡 **Command-Line Usage**

### 1. Serve Mode

Host a directory on the network.

```bash
synclite serve ./project
````

* Starts a small TCP listener on port (e.g.) `8080`.
* Loads `.syncignore` and `.synclite/state.json` (if it exists).
* If no `.synclite/workspace.json` exists, generates one automatically with a unique `workspace_id`.
* Waits for connections and announces workspace identity.
* Periodically scans the directory for changes (or reacts to sync requests).
* Shares file hashes and metadata with connected peers.

### 2. Connect Mode

Connect to a peer and sync your local directory with theirs.

```bash
synclite connect 192.168.1.42 ./my-copy
```

* Connects to the host via TCP.
* If `./my-copy` is empty, it becomes a new clone of the host workspace.
* If it already contains `.synclite/workspace.json`, SyncLite verifies the `workspace_id` before syncing.
* Exchanges directory metadata (file names, sizes, hashes).
* Determines which files need updating.
* Requests and applies only changed files.
* Updates its local `.synclite/state.json`.

## 🧩 **Workspace Identity System**

Each synced folder contains a hidden `.synclite/workspace.json` file that uniquely identifies the workspace across all peers:

```json
{
  "workspace_id": "a5f8e912-2b5c-4a6a-91f3-832a11c2c56d",
  "workspace_name": "Project",
  "created_at": "2025-11-01T14:52:12Z",
  "owner": "192.168.1.10",
  "peers": ["192.168.1.10"]
}
```

* Peers use the `workspace_id` to ensure they belong to the same sync group.
* Local folder names can differ (e.g., `./Project` vs. `./MyCopy`).
* If a peer connects with a mismatched or missing `workspace_id`, SyncLite prompts whether to:

  * **Join the existing workspace** (if empty folder)
  * **Abort the connection** (if conflicting workspace)

This system ensures consistency across all peers — independent of folder names or paths.

## 📂 **Sync Cycle (Step-by-Step)**

1. **Handshake**

   * Client connects, sends “Hello” with its peer ID and workspace metadata.
   * Server acknowledges and sends its file manifest and `workspace_id`.

2. **State Exchange**

   * Each peer sends a JSON map of filenames and hashes:

     ```json
     {
       "src/main.rs": "abc123",
       "Cargo.toml": "def456"
     }
     ```
   * The manifest is compared to find:

     * New files
     * Modified files
     * Deleted files

3. **File Transfer**

   * Only differing files are sent.
   * Framing structure:

     ```
     [filename_length][filename][file_size][file_bytes]
     ```

4. **Conflict Resolution**

   * If both sides modified the same file (hash mismatch on both):

     * Option 1: “Newest file wins” (compare timestamps).
     * Option 2: Create `filename.conflict` on both machines.

5. **State Update**

   * After syncing, `.synclite/state.json` is updated:

     ```json
     {
       "files": { "src/main.rs": "abc123", ... },
       "last_sync": "2025-11-01T14:23:52Z"
     }
     ```

## 🧱 **.syncignore System**

Each shared directory can include a `.syncignore` file, similar to `.gitignore`:

```
# Ignore build artifacts
target/
dist/

# Ignore dependencies
node_modules/

# Ignore secrets
.env
.env.local

# Ignore logs
*.log
```

* Patterns can include `*`, `**`, and `/`.
* `#` comments and empty lines are ignored.
* Negations (`!pattern`) can re-include specific paths.
* SyncLite skips ignored files entirely (they’re not hashed or transferred).

## 🧠 **Peer Management & Server Reassignment**

SyncLite supports **multiple peers** in the same network session:

* The original host is the **leader**.
* Each peer keeps a small peer list (`.synclite/peers.json`):

  ```json
  {
    "leader": "192.168.1.42",
    "peers": ["192.168.1.12", "192.168.1.15"]
  }
  ```
* If the leader disconnects:

  * The remaining peers elect the **next leader** (based on IP order or uptime).
  * The new leader announces itself to others.
  * Sync continues seamlessly without user intervention.

This prevents the “one host dies, everyone stops” issue.

## 🔁 **Example Scenario**

**1️⃣ You host a folder:**

```bash
synclite serve ~/Projects/MyApp
```

**2️⃣ Two teammates join:**

```bash
synclite connect 192.168.1.10 ~/Desktop/MyCopy
```

**3️⃣ Everyone makes edits locally:**

* You modify `src/main.rs`
* Alice adds `README.md`
* Bob changes `.env` (which is ignored)

**4️⃣ SyncLite compares and resolves:**

* `.env` skipped (ignored)
* `README.md` → downloaded
* `src/main.rs` → newest version propagated
* `.synclite/state.json` updated for all

**5️⃣ You close your CLI:**

* Alice and Bob remain connected.
* Alice becomes new leader automatically.
* Sync continues on LAN.

## 🧩 **How SyncLite Complements Git**

| Git                            | SyncLite                                |
| ------------------------------ | --------------------------------------- |
| Tracks **version history**     | Tracks **current state**                |
| Needs **manual commits**       | Syncs **automatically**                 |
| Cloud or remote-based          | Local **LAN-based**                     |
| Focuses on **code evolution**  | Focuses on **collaboration speed**      |
| Great for teams using branches | Great for local dev or fast prototyping |

Think of SyncLite as **“instant Git pull”** for your LAN — perfect for hackathons, classrooms, or rapid co-editing without a central server.

## 🔒 **Optional Future Enhancements**

* Conflict auto-merging with diffs
* File watching (detect changes instantly with polling)
* Peer discovery via UDP broadcast
* Async version using `Tokio`
* Encryption layer (TLS or XOR obfuscation)
* Git plugin integration (e.g. `git synclite-push` for local network mirroring)

## 🚀 **Summary**

| Category               | Description                                                                           |
| ---------------------- | ------------------------------------------------------------------------------------- |
| **Name**               | SyncLite                                                                              |
| **Language**           | Rust (standard library only)                                                          |
| **Purpose**            | Direct, LAN-based file synchronization                                                |
| **Key Features**       | TCP-based transfer, checksum validation, `.syncignore`, state tracking, peer failover |
| **Workspace Identity** | Unique `.synclite/workspace.json` ensures consistent workspace mapping                |
| **Design Philosophy**  | Minimal, transparent, Git-like for simplicity and integrity                           |
| **Ideal Use Case**     | LAN collaboration, dev teams, rapid iteration                                         |
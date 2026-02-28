<div align="center">
  <img src="banner.png" width="800" alt="Cortex v3.1 Banner" />
  <h1>Cortex v3.1 RMVM</h1>
  <p><strong>A deterministic, capability-secured relational memory virtual machine for verifiable AI agent memory.</strong></p>
  
  > [!IMPORTANT]
  **Experimental:** This project is currently in an experimental state and requires further refinements and testing before it should be considered production-ready.
  

  <p>
    <a href="https://github.com/vinzify/Cortex-v3.1-RMVM---Relational-Memory-Virtual-Machine/releases"><img src="https://img.shields.io/github/v/release/vinzify/Cortex-v3.1-RMVM---Relational-Memory-Virtual-Machine?style=flat-square&color=blue" alt="Current Release"></a>
    <a href="https://tauri.app/"><img src="https://img.shields.io/badge/Built_with-Tauri_%7C_Rust-orange?style=flat-square" alt="Built with Tauri & Rust"></a>
    <a href="https://github.com/vinzify/Cortex-v3.1-RMVM---Relational-Memory-Virtual-Machine/blob/main/LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square" alt="License: MIT"></a>
    <a href="https://github.com/vinzify/Cortex-v3.1-RMVM---Relational-Memory-Virtual-Machine/stargazers"><img src="https://img.shields.io/github/stars/vinzify/Cortex-v3.1-RMVM---Relational-Memory-Virtual-Machine?style=social" alt="GitHub stars"></a>
  </p>
</div>

Cortex v3.1 is a formal memory system specification and implementation. It transforms agent memory from malleable, hallucination-prone text into **verifiable data**, and shifts agent reasoning from probabilistic narration into **deterministic execution** over capability-secured memory. 🧠✨

When Cortex v3.1 remembers something, it provides cryptographic proof of lineage back to tamper-evident evidence. 🔗🛡️

## ✨ Key Features

*   **🧠 Deterministic RMVM Logic:** Moves beyond free-form LLM memory writing. The model emits a bounded execution plan in strict SSA-style bytecode, which the kernel executes over verifiable memory.
*   **🛡️ Capability-Secured Handles:** The model only ever sees public `HandleRef` and `SelectorRef` objects. Private memory IDs, digests, and capability tokens are isolated within the kernel.
*   **🔗 Cryptographic Lineage:** Every factual output is bound to a `VerifiedAssertion`. Using SHA-256 Merkle roots, assertions are bit-identical across independent kernels for the same inputs.
*   **⚠️ Hallucination-Resistant by Construction:** Render factual text using templates bound 1:1 to assertion fields. The "narrative channel" is restricted by a strict grammar to prevent the introduction of new "facts".
*   **📉 Static Cost Guard (COST_GUARD):** Plan DAGs are analyzed before execution. Expensive or non-terminating plans are rejected with structured pruning hints.

## 🚀 Quick Start

Cortex v3.1 is designed to be framework-agnostic with high-performance SDKs and a strict gRPC interface. 🚀⚡

### 📋 Prerequisites
- [Rust](https://www.rust-lang.org/tools/install) (2024 Edition) 🦀
- [Protobuf Compiler](https://grpc.io/docs/protoc-installation/) 🛠️

### ⚙️ Installation & Validation
```bash
# Clone the repository
git clone https://github.com/vinzify/Cortex-v3.1-RMVM---Relational-Memory-Virtual-Machine.git
cd Cortex-v3.1-RMVM---Relational-Memory-Virtual-Machine

# Run the workspace tests
cargo test --workspace

# Run the conformance runner to verify implementation integrity
cargo run -p rmvm-tests --bin conformance_runner -- check
```

## 🏗️ The RMVM Model

The Cortex v3.1 architecture is built on three non-negotiable pillars:

### 1. 🗃️ The Event Ledger
An immutable, content-addressed append-only log of all user inputs, tool outputs, and agent actions. All memory objects are derived from this ledger via provenance anchors.

### 2. ✅ Verified Assertions
Factual text is never "recalled"; it is **rendered** from assertions. If a provenance anchor is invalidated (e.g., corrupted evidence), all derived memory objects are automatically demoted and quarantined.

### 3. 🔥 Bounded Execution
Relational memory operations are restricted to prevent infinite loops or explosive joins:
- No recursion permitted.
- Max join depth = 3.
- Max ops per plan = 128.

## 📦 Packages

| Package | Language | Description |
| :--- | :--- | :--- |
| `rmvm-proto` | Rust / Protobuf 🦀 | Core contract & wire-level protocol. |
| `rmvm-kernel` | Rust 🦀 | Reference RMVM kernel implementation. |
| `@cortex/rmvm-sdk` | TypeScript 🟦 | Client SDK for web & Node.js agents. |
| `cortex-rmvm-sdk` | Python 🟨 | Client SDK for research & engineering. |

## 🛠️ Development

Cortex is built for extreme performance and memory safety using Rust. 🔨💻

```bash
# Build all crates
cargo build --release

# Generate gRPC bindings
# (Triggered automatically during cargo build)
```

For more detailed guides, see:
- 🟦 TypeScript: [`docs/quickstarts/hello_memory_typescript.md`](docs/quickstarts/hello_memory_typescript.md)
- 🟨 Python: [`docs/quickstarts/hello_memory_python.md`](docs/quickstarts/hello_memory_python.md)
- 📄 Spec: [`docs/cortex_v3_1_rmvm_spec (1).md`](docs/cortex_v3_1_rmvm_spec%20(1).md)

---
## 📜 License
**License:** MIT

---
💖 **Support the Project**

If you want to support its continued development, consider sending an ETH donation: 
`0xe7043f731a2f36679a676938e021c6B67F80b9A1`

---
*Cortex v3.1 RMVM - Building the substrate for verifiable intelligence.*

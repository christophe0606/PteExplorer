# PTE Explorer

A native Rust explorer for ExecuTorch `.pte` files, rendered on the GPU with
[GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) 0.2.2.
Supports Ethos-U55, U65 and U85. No Python, ExecuTorch runtime, Vela installation
or attached NPU is needed. Files are inspected locally without executing code.

## Example model provenance

The local `model.pte` example is covered by the **Apache License, Version 2.0**
([license text](LICENSE)). It is an ExecuTorch conversion of the
`yolo-fastest_192_face_v4.tflite` face-detection network from the Arm ML Eval Kit.
The TFLite model was extracted without modification from
`object-detection/src/yolo-fastest_192_face_v4.tflite.cpp` in the public
[Arm ML Eval Kit CMSIS examples](https://github.com/Arm-Examples/mlek-cmsis-pack-examples/tree/main/object-detection),
then converted to ExecuTorch. See [model attribution and source links](THIRD_PARTY.md#example-model)
for details.

## Build from a fresh clone

Install Git, **CMake 3.24+**, and current stable Rust using
[rustup](https://rustup.rs/). `Cargo.lock` pins dependencies. The first build
downloads crates and takes several minutes; allow several GB of disk and RAM.
The current development toolchain is Rust 1.99.1.

### Windows

Install Visual Studio 2022 Build Tools or Community with **Desktop development
with C++**, MSVC and a Windows 10/11 SDK. Use its x64 Native Tools Command Prompt
or Developer PowerShell. Select Rust's MSVC toolchain with
`rustup default stable-x86_64-pc-windows-msvc`. Running requires Direct3D 11.

```powershell
git clone https://github.com/christophe0606/PteExplorer.git PteExplorer
cd PteExplorer
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --config Release
ctest --test-dir build -C Release --output-on-failure
.\build\cargo\release\pte-explorer.exe C:\path\to\model.pte
```

No model is bundled with a fresh clone; use a local `.pte` file.
The Windows GUI executable opens without a terminal window. You can launch it
directly from Explorer and use **Open .pte…**. Main and detached windows size
themselves to the primary monitor's usable area, accounting for Windows display
scaling and leaving room for the native title bar and taskbar.
CMake selects one Windows SDK `fxc.exe` for GPUI's Release shader compilation.
To override discovery, configure with
`-DPTE_FXC_EXECUTABLE="C:/path/to/Windows SDK/bin/x64/fxc.exe"`.
For direct Cargo Release builds, set `GPUI_FXC_PATH` to that executable's full
path if multiple SDK shader compilers are on `PATH`.

### macOS

Install full Xcode, launch it to accept its license and install components, and
select its developer directory. GPUI compiles Metal shaders; command-line tools
alone may be insufficient. Install CMake. A Metal-capable Mac is required.

```sh
xcode-select --install
sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer
# If Xcode distributes Metal separately:
# xcodebuild -downloadComponent MetalToolchain
git clone https://github.com/christophe0606/PteExplorer.git PteExplorer
cd PteExplorer
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --config Release
ctest --test-dir build -C Release --output-on-failure
./build/cargo/release/pte-explorer /path/to/model.pte
```

### Linux (Ubuntu / Debian)

Install these development libraries and your Vulkan GPU driver. Both X11 and
Wayland are enabled. Running the GUI requires a graphical session.

The Linux GUI dependency configuration pins `libc` to 0.2.189: GPUI 0.2.2
indirectly depends on `xattr` 0.2.3, which uses the Linux `ENOATTR` alias
removed in `libc` 0.2.190. Keep this pin until that dependency is updated.

```sh
sudo apt-get update
sudo apt-get install -y build-essential cmake clang libclang-dev pkg-config \
  libssl-dev libfontconfig1-dev libfreetype-dev libwayland-dev \
  libx11-dev libx11-xcb-dev libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev \
  libvulkan-dev libzstd-dev
git clone https://github.com/christophe0606/PteExplorer.git PteExplorer
cd PteExplorer
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build
ctest --test-dir build --output-on-failure
./build/cargo/release/pte-explorer /path/to/model.pte
```

`cmake --build build --target run` opens the app without an initial file.
`cmake --install build --prefix dist --config Release` installs to `dist/bin`.
CMake invokes Cargo. No generated schema compiler or C++ project is required.
Omit `-DCMAKE_BUILD_TYPE=Release` for debug. Multi-configuration generators use
`--config Release` for Cargo release; other configurations use debug.

For a lightweight parser/JSON inspector without GPUI or a display:

```sh
cmake -S . -B build-core -DPTE_GUI=OFF -DCMAKE_BUILD_TYPE=Release
cmake --build build-core --config Release
./build-core/cargo/release/pte-explorer --inspect /path/to/model.pte
```

Direct Cargo commands also work:

```sh
cargo run --release -- /path/to/model.pte
cargo test --locked --no-default-features
cargo test --locked --features ui-tests --bin pte-explorer
cargo run --no-default-features -- --inspect /path/to/model.pte
```

## Explore a file

* **Open .pte…** selects a file; parsing runs on a background thread.
* **File & method info** shows version, methods, inputs/outputs, planned memory,
  segments, delegate compile specs and any recorded Ethos/Vela configuration.
* Use the **Method** dropdown to select any method, including scalar configuration
  methods. The selected name is always shown in the dropdown.
* The canvas connects operators directly from left to right. Intermediate tensors
  appear as named inputs and outputs in operator details, without extra graph
  nodes. Method inputs and outputs remain at the graph boundaries. Dependencies
  follow the latest preceding producer, including in-place writes. Repeated
  output-buffer/return aliases appear once in the details. Green cards identify method
  inputs and blue cards identify method outputs. Weight/bias shapes appear on
  operator cards; named scalar/list parameters and collapsible constant-tensor
  details appear in the right pane rather than as separate graph nodes.
  Kernel argument names and directions use 2,228 bundled ATen and ExecuTorch
  quantization schemas, matched by full operator/overload name and argument count.
  Ethos delegate roles use Vela I/O descriptors and backend argument order.
  Control instructions retain execution/jump arrows. Unsupported kernel schemas
  keep undirected tensor/value links and an explicit availability explanation.
  Fused delegate-internal PyTorch operators are generally absent from PTEs.
* Drag to **pan**; wheel or **− / +** to zoom; **Fit** to center the graph;
  each method uses its fitted camera from the first graph frame, after measuring
  the available canvas, so both inputs and outputs are visible.
  **100%** for readable 16 px titles and 13 px labels. Wheel zoom preserves the
  point under the pointer.
* **Hide sidebar / Show sidebar** controls the right detail pane. Clicking empty
  canvas clears the selected node and hides the pane; dragging still pans without
  clearing selection. Clicking a node opens its details and keeps that node inside
  the remaining canvas. Opening or resizing the pane pans as needed, preserving
  zoom unless the selected node is too large to fit. **File & method info** opens
  the metadata pane again.
* Click operators/values for datatype, shape, dimension order, allocation,
  scalar/list contents and other details in the right pane. Details use property
  cards with bold field labels; click a section heading to collapse or expand it.
* Click a tensor's **ExecuTorch arena** allocation to open an independent arena
  view, focused on that tensor's data span. **Arenas** opens the method's planned
  buffers overview. Capacity rows and tensor rows share byte-offset axes; colors
  identify arena IDs. Use **Fit selection**, **Fit arena**, **Fit map**, pan and zoom.
  Each arena is a planned mutable-memory buffer supplied by the runtime. Its ID
  is separate from NPU region indices. A method can have several arenas; the
  method summary lists IDs and capacities. Tensor addresses are arena base + byte
  offset, with runtime bases unavailable in the PTE. Overlap may represent aliases
  or reuse at different times; tensor byte spans exclude allocator padding and
  do not establish live allocations or free space. Unknown sizes appear as markers.
* Click an **Ethos-U55/U65/U85** delegate for Vela disassembly. **Show delegate overview**
  shows hardware configuration, compile specs, block/scratch sizes, I/O descriptors
  and observed NPU memory regions.
* Click any recognized command for an expanded **Selected instruction** card:
  its function, meaning, interpreted value and units, execution effect, and
  related settings. This covers every recognized U55/U65/U85 opcode. Settings
  explain what they change without starting work; operation commands explain
  what they launch, wait for, or control. Cards below show accumulated state in
  linear stream order. Raw words and decoded bitfields start collapsed.
  **All commands** and **Operations only**
  are explicit filter choices; a check mark identifies the active filter and a
  count states how many commands are shown. A draggable scrollbar and **First** /
  **Last** buttons navigate long streams. Detail panes also have visible scrollbars.
  **Copy details** copies the current detail view, including collapsed sections.
* **DMA details** explain the selected instruction first, followed by the copy
  direction, bytes per transfer, and separate source and destination cards.
  `SRC_REGION` selects the source buffer and access modes; `SRC` sets its
  starting byte offset; `LEN` sets bytes per transfer. Destination settings
  follow the same distinction. Buffer roles (weights/constants,
  scratch/activations, fast scratch) are identified as usual Vela assignments.
  Each external endpoint names its **NPU region**, byte offset and matching
  **NPU_REGIONCFG_n** driver setting. A collapsible explanation relates base
  pointers to runtime addresses and arena allocations. Advanced register fields
  start collapsed. Configuration commands and waits are distinguished from copy
  launches; unrelated tensor-register state is omitted from DMA details.
* **Double-click a DMA buffer address** in either endpoint card to open a separate
  region-map window centered on that command's source or destination span.
  **Fit selection** returns to the selected span; **Fit map** shows all recorded
  spans. Unknown transfer lengths appear as address markers. Strided/indexed
  spans show the configured offset and per-transfer length, not all repeated
  accesses. Internal NPU memory is shown separately from external NPU regions.
* **Region map** is an additional pan/zoom canvas of recorded I/O spans, DMA reads
  and writes, and weight/scale buffers. Click a bar to inspect its region, offset,
  length and related transfer. Each NPU region has a consistent color in the map,
  legend and DMA address links. Selection adds a white outline without changing
  the region color. Use **Fit map**, **100%**, wheel zoom or drag to pan.
  The axis shows offsets relative to each region's runtime base. Overlapping spans
  may be reused across commands; the map does not claim allocation lifetimes or
  physical addresses. DMA bars show per-transfer spans, with stride/index limits
  explained in their details. **Command stream** returns to disassembly.
* **Double-click any graph node** to open its details in a separate window.
  Ethos delegate nodes open an independent command stream with instruction details
  on the right. Each window retains the original data if the main window opens
  another file. A single click continues to show details in the main window.

## Interpretation and format support

Supports ExecuTorch **ET12**, inline and extended-header segmented delegate data,
named-data external Vela blocks stored in the PTE, scalar/list methods, and all
current instruction union kinds. Offset, vector and block reads are checked.
Unsupported delegate formats show warnings. Model-embedded file paths are not
followed automatically.

Vela `vela_bin_stream` and `COP1` use architecture-specific opcode/bitfield tables
for U55/U65 and U85. Both historical 4D and newer 6D I/O descriptors work. Unknown
opcodes retain raw words and warnings; unknown products/framing show delegate
warnings. This is a static inspector, not a simulator.

NPU state comes from preceding writes. Dimensions describe stripes/tiles and may
differ from full method tensors. Unwritten registers are unknown. DMA of compressed
weights/arbitrary bytes usually does not encode tensor dimensions; byte lengths,
region-relative addresses and stride/index fields are shown. Branch-dependent
execution and runtime state cannot be recovered by linear disassembly alone.
U85 chained tensors use internal buffers, which are distinguished from external
base-pointer regions. Input chain dimensions use the last preceding producer's
output shape in linear stream order.

An external NPU address is the runtime base address of the selected NPU region
plus its byte offset. `BASEP[n]` is the app's shorthand for that complete base
address, rather than an individual low/high hardware register. Vela's region
index matches the runtime base-address slot and `NPU_REGIONCFG_n` driver setting.
The setting's value selects access configuration; it is not an address or a new
region index. NPU region indices and ExecuTorch arena IDs are separate identifiers.
Region roles show Vela conventions and observed uses. Physical addresses,
SRAM/DRAM placement and AXI mapping are chosen by the runtime and firmware.
Compile specs can record configuration names/flags. A recorded
`--config=.../vela.ini` path does not embed that INI file. Omitted configuration
cannot be recovered reliably.

## Development

`src/pte.rs`: program reader. `src/vela.rs`: command decoder/state.
`src/graph.rs` and `src/graph/`: layout/camera and argument roles.
`src/schemas.rs`: bundled operator signatures. `src/ui.rs` and `src/ui/`: GPUI windows and panels.
`src/arena.rs`: planned arena capacities, tensor footprints and operator uses.
`tests/core.rs` covers architecture differences, high DMA addresses, state history,
dimensions, external blocks, old/new I/O, multiple methods, truncated input and
camera invariants, mixed CPU/delegate tensor flow, output aliases and in-place
writes. Every bundled operator signature is parsed in a catalog regression.
A regression inspects `model.pte` if supplied locally. Set
`PTE_EXPLORER_YAMNET_MODEL` to the reported YAMNet PTE path to additionally check
its 84 directly connected operations, two method boundaries, 28 constant tensors,
and 85 planned tensor spans in the 807,168-byte arena. Synthetic tests need no user model.
GPUI event tests exercise named parameters and constant sections, panning, zoom controls, value
selection, copying details, the method dropdown, collapsible property cards,
scrollbar clicking/dragging in a 1,000-command stream, explicit filters, region-map
selection/pan/zoom, double-clicking operator/value/delegate nodes to open separate
detail windows, and independent selection in a detached stream window.
They also check first-frame camera/node geometry, sidebar toggling, blank clicks
versus dragging, and selected-node visibility when opening/resizing the sidebar,
including graph boundaries sharing the same tensor ID.
These interaction tests use GPUI's test platform and run without a visible
desktop session, including on GitHub Actions.
Arena regressions cover multiple buffers, reserved entry 0, reused offsets,
unknown/overflowing sizes, bounded shapes, initialized mutable tensors, and
click-to-focus allocation links above 4 GiB with fit/copy controls.
DMA regressions cover all three architectures, internal memory, missing settings,
historical addresses and lengths, address double-clicks and transfer-focused maps
at offsets above 4 GiB.
Resize checks cover 700–1600 px windows, including a 700×500 window.
CTest runs these when the GUI is enabled. Cross-platform build jobs are in
`.github/workflows/build.yml`; their presence alone does not verify all hosts.
When developing on Windows, validate Windows only, as specified in `AGENTS.md`.
See [THIRD_PARTY.md](THIRD_PARTY.md) for source provenance.

# Validation status

This records observed evidence and remaining checks. It is not a claim that every
supported operating system has been tested.

## Implemented scope

| Requirement | Implementation | Evidence |
| --- | --- | --- |
| Rust GPU GUI and CMake build | GPUI 0.2.2; `CMakeLists.txt` invokes locked Cargo builds | Windows CMake Debug and Release GUI builds passed; native startup and Release model inspection passed |
| Windows desktop startup | Windows GUI subsystem; DPI-aware work-area bounds for main and detached windows, with native-frame margins | Release PE header reports GUI subsystem (2); placement regression covers small/scaled and offset work areas |
| Fresh-clone instructions | `README.md`, platform prerequisites and commands | Windows CMake configure, Release build and CTest passed; other native hosts were not tested |
| Open PTE | Native picker, command-line file, background parse, errors | Supplied model opened through command-line/native application; reader regression |
| Explore metadata and every method | Method dropdown; collapsible file/method property cards | Two-method parser fixture plus GPUI dropdown and section-collapse tests |
| Operator graph | Operators connect directly; method inputs/outputs remain as boundaries, intermediate tensors stay in details; schemas label ports and parameters; latest-producer dependencies handle in-place writes | Synthetic mixed CPU/delegate and Vela ABI regressions; reported YAMNet PTE displays 84 operators and two boundaries with 85 directed links, all connected to its method input |
| Planned arena visualization | Tensor allocation links open a separate pan/zoom view fitted to that tensor; arena capacities, tensor spans, operator uses and fit controls; IDs separate from NPU regions | YAMNet records Arena 1 with capacity 807,168 bytes and 85 tensor allocations; synthetic multi-arena/reuse/unknown-size tests and GPUI single-click navigation above 4 GiB, visible focus bars, Fit arena / Fit selection / Fit map and copying |
| Pan, zoom, Fit, 100% | Camera transforms and mouse/button handlers | Camera invariants and GPUI simulated drag/wheel/button input passed |
| Graph sidebar and selection | Hide/Show controls; empty-canvas clicks clear selection and hide details; node clicks open details and reveal the selected node in the measured canvas | GPUI regression covers a node beyond the future sidebar boundary, manual zoom preservation, 700–1300 px resizes, panning without deselection, metadata reopening and duplicate tensor IDs at graph boundaries |
| Initial graph framing | Camera fitting and node/edge layout use the measured canvas during the same prepaint step | GPUI regression checks node bounds against the fitted camera on the first graph draw and confirms the camera stays unchanged afterward |
| Operator/tensor detail pane | Collapsible property cards with bold labels for datatype, shape, dimension order, allocation and values | Parser checks, collapse/layout and selection interaction tests |
| Ethos-U55/U65/U85 disassembly | COP1 framing and separate architecture opcode/field tables | Synthetic fixtures exercise all three products and differing opcodes; supplied U85 model decodes 3,465 commands without unknown opcodes |
| Instruction meanings, tensors, transfers | Every recognized opcode has a function, meaning, interpreted value/units, execution effect and related settings in an expanded first card; state history, tensor tiles, DMA source/destination/length/strides | Catalog test covers every recognized U55/U65/U85 opcode; integration checks distinguish region/offset/length, dimensions, byte versus kernel strides, scaling, configuration/start/wait and U85 resize STEP fields; historical state, high-address DMA, chaining and broadcast tests |
| Explain NPU regions | Observed uses, conventional roles, I/O descriptors and runtime-base explanation | Region tests, segmented/external-block fixture; internal chaining buffers distinguished |
| Understand DMA buffers and driver settings | Selected instruction explanation, direction and byte count; source/destination cards with usual buffer role, NPU index, offset, recorded buffer sizes and NPU_REGIONCFG_n; advanced fields collapsed | All-product regressions check historical state, missing settings, internal versus external memory, and configuration/start/wait distinctions |
| Locate DMA endpoints graphically | Double-click an address to open an independent region map fitted to that command's configured span; stable colors by NPU region | GPUI input test opens source and destination maps above 4 GiB, checks selected command/length, centered visible bars, Fit map / Fit selection and copying; eight unique region colors |
| Region map | Additional pan/zoom canvas of recorded I/O, DMA and weight/scale spans; structured selection details | High-address DMA range checks plus GPUI map selection, copy, pan, zoom and Fit tests |
| Long command navigation | Visible draggable scrollbar, First/Last controls, explicit All commands / Operations only choices | GPUI test navigates and filters a 1,000-command stream, including rail clicks and thumb drags |
| Double-click node details | Separate structured operator/value detail windows; independent Ethos command stream with its own right pane | GPUI double-click tests verify exactly one window, retention after model changes and independent command selection, including canvas resizing after the first click |
| Show recorded Ethos/Vela configuration | Compile specs, COP1 product/MACs, flags, scratch and blocks | Supplied model records ethos-u85-256, Ethos_U85_SRAM_MRAM and Shared_Sram |

`cargo test --locked --no-default-features` runs sixteen core tests (two catalog
unit tests and fourteen integration tests). The local-model test skips when
`model.pte` is absent. The YAMNet regression skips unless
`PTE_EXPLORER_YAMNET_MODEL` names the reported PTE; the recorded Windows run set
that variable and validated the supplied file.
`cargo test --locked --features ui-tests --bin pte-explorer` runs fourteen UI tests:
two placement/port-role regressions and twelve tests using GPUI's test platform. These check
placement, layout and input dispatch; they do not capture or visually inspect
GPU-rendered pixels. Placement checks cover both initial window sizes on small
displays, 200% scaling, offset work areas and larger displays.
The resize test checks that the right detail pane, stream controls, Fit, and 100%
remain within window bounds at widths of 700, 900, 1200, and 1600 logical pixels,
including a 700×500 window.

`cargo fmt --check` and Clippy with warnings denied passed for the parser/core.
CMake keeps core-test artifacts in a separate Cargo directory so testing the
headless binary cannot replace the desktop executable.
The final Windows `ctest --test-dir build -C Release --output-on-failure` run
passed both suites (sixteen core tests and fourteen UI tests). The Release
executable inspected the supplied model and remained running during a five-second
native startup check, without stderr output.
The reported YAMNet PTE also passed Release inspection and the same native
startup check; its graph regression was enabled during the recorded test run.

Windows Release builds precompile GPUI's shaders. CMake passes one discovered
Windows SDK `fxc.exe` explicitly through `GPUI_FXC_PATH`, avoiding GPUI 0.2.2's
handling of multiple `where.exe` results as one executable name. The corrected
Release build passed with Windows SDK 10.0.26100.0.

## Limits and manual verification

* Visually inspect the GPU-rendered main/delegate/detached windows, including
  scrolling and narrow-window layout. This session has no callable desktop
  screenshot/input tools. A responsive window alone does not prove visual quality.
* Validation from Windows is limited to Windows by user instruction, recorded in
  `AGENTS.md`. Do not attempt Linux/macOS builds or runs from this environment;
  do not use Docker. Cross-platform CMake configuration and instructions remain
  maintained. Other native hosts have not been validated here.
* Exercise real U55 and U65 PTE files. Their architecture-specific synthetic
  command fixtures pass; the supplied real model targets U85.
* Expand semantic coverage for branch-dependent command streams. Current state
  reconstruction is explicitly linear, with unknown/runtime-dependent information
  labeled in the UI and README.

No physical memory addresses or missing Vela INI contents are inferred from
region numbers or filenames. The inspector reports information present in the
PTE and marks unavailable information explicitly.

Ethos argument roles use the backend ABI (inputs followed by outputs, with counts
from Vela's I/O descriptors). Kernel calls use bundled schemas with exact overload
and argument-count matching; unsupported signatures remain explicitly unclassified.
The method graph versions writes in linear instruction order; it does not execute
branches or reconstruct implicit views solely from overlapping memory allocations.
Arena rows show recorded planned capacities and calculated dense tensor data
footprints, excluding alignment/padding. Bounded dynamic shapes use their recorded
upper bounds. Unbounded shapes, non-zero storage offsets, unsupported element
sizes and size overflows retain offset markers without inferred lengths. Arena
overlaps do not establish tensor liveness or simultaneous memory usage.
Region bars are recorded spans rather than an
allocation/liveness analysis; DMA lengths are per transfer, so strided/indexed
accesses may extend beyond a bar. Selecting a DMA span retains its source and
destination details.

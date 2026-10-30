# Third-party provenance

## Example model

The local example file `model.pte` is covered by the **Apache License, Version 2.0**.
The license text is included in [LICENSE](LICENSE).

It is an ExecuTorch conversion of the original Arm ML Eval Kit object-detection
network, `yolo-fastest_192_face_v4.tflite`. The TFLite model was extracted,
without modification, from
[`object-detection/src/yolo-fastest_192_face_v4.tflite.cpp`](https://github.com/Arm-Examples/mlek-cmsis-pack-examples/blob/main/object-detection/src/yolo-fastest_192_face_v4.tflite.cpp)
in the public
[Arm ML Eval Kit CMSIS examples](https://github.com/Arm-Examples/mlek-cmsis-pack-examples/tree/main/object-detection),
then converted to ExecuTorch to produce `model.pte`.
The extraction preserved the original TFLite bytes; `model.pte` is the converted
ExecuTorch artifact.

The Arm ML Eval Kit also provides the network in its
[video ML model template](https://github.com/Arm-Examples/cmsis-mlek/tree/main/template/video/ML/model).

## Format and opcode provenance

`src/generated/opcodes.rs` contains opcode/field metadata derived from Arm Vela,
under Apache-2.0. Copyright 2020–2026 Arm Limited and/or affiliates.
The license text is included at [licenses/Vela-LICENSE.txt](licenses/Vela-LICENSE.txt).

* [U55/U65 definitions](https://github.com/nxp-imx/ethos-u-vela/blob/lf-6.18.20_2.0.0/ethosu/vela/ethos_u55_regs/ethos_u55_regs.py)
* [U85 interface](https://github.com/nxp-imx/ethos-u-vela/blob/lf-6.18.20_2.0.0/ethosu/regor/architecture/ethosu85/ethos_u85_interface.hpp)
* [Vela license](https://github.com/nxp-imx/ethos-u-vela/blob/lf-6.18.20_2.0.0/LICENSE.txt)

`tools/generate-opcodes.ps1` regenerates the table from local copies of those files
at `research/registers.py` and `research/u85.hpp`. `research/` is ignored. This
PowerShell maintenance tool is not needed for ordinary builds.

Instruction explanations and interpreted values in `src/vela/descriptions.rs`
also follow Vela's command generators:

* [U55/U65 command generator](https://github.com/nxp-imx/ethos-u-vela/blob/lf-6.18.20_2.0.0/ethosu/vela/register_command_stream_generator.py)
* [U85 command generator](https://github.com/nxp-imx/ethos-u-vela/blob/lf-6.18.20_2.0.0/ethosu/regor/architecture/ethosu85/ethos_u85_register_cs_generator.cpp)

The generated U85 table uses canonical `RESIZE_X_STEP` / `RESIZE_Y_STEP` names
to match the interface's instruction structures and decode their fields.

The reader follows Meta's BSD-licensed
[PTE schema](https://github.com/pytorch/executorch/blob/main/schema/program.fbs),
[scalar types](https://github.com/pytorch/executorch/blob/main/schema/scalar_type.fbs),
[extended header](https://github.com/pytorch/executorch/blob/main/schema/extended_header.h),
and [VelaBinStream](https://github.com/pytorch/executorch/blob/main/backends/arm/runtime/VelaBinStream.h).
No ExecuTorch runtime source is compiled into this project.
Arena IDs and byte-offset interpretation follow the schema and runtime
[HierarchicalAllocator](https://github.com/pytorch/executorch/blob/main/runtime/core/hierarchical_allocator.h).

`src/generated/operator-schemas.txt` contains 2,228 ATen and quantization operator
signatures derived from Netron's MIT-licensed
[PyTorch metadata](https://github.com/lutzroeder/netron/blob/main/source/pytorch-metadata.json),
with ExecuTorch's
[quantization signatures](https://github.com/pytorch/executorch/blob/main/kernels/quantized/quantized.yaml)
taking precedence. License texts are included in
[Netron-LICENSE.txt](licenses/Netron-LICENSE.txt) and
[ExecuTorch-LICENSE.txt](licenses/ExecuTorch-LICENSE.txt).
`tools/generate-operator-schemas.ps1` regenerates the catalog from local copies at
`research/pytorch-metadata.json` and `research/quantized_functions.yaml`.
These maintenance inputs and Python/PyTorch are not required to build or run.

GPUI and other Rust dependencies retain the licenses in their crate distributions.

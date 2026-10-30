// Small reproduction of YAMNet's CPU/delegate/CPU boundary and out aliases.
pub fn method() -> pte_explorer::pte::Method {
    use pte_explorer::pte::{Delegate, Instruction, Method, Tensor, Value};
    let tensor = |dtype: &str, shape: &[i32], data_buffer| Value {
        kind: "Tensor".into(),
        description: format!("{dtype} {shape:?}"),
        references: vec![],
        tensor: Some(Tensor {
            dtype: dtype.into(),
            shape: shape.to_vec(),
            dim_order: (0..shape.len() as u8).collect(),
            storage_offset: 0,
            data_buffer,
            allocation: None,
            shape_dynamism: 0,
            name: String::new(),
        }),
    };
    let scalar = |kind: &str, description: &str| Value {
        kind: kind.into(),
        description: description.into(),
        tensor: None,
        references: vec![],
    };
    let mut values = vec![
        tensor("float32", &[1, 96, 64], 0),     // 0: method input
        tensor("float32", &[32, 1, 3, 3], 1),   // 1: weights
        tensor("float32", &[32], 2),            // 2: bias
        tensor("int16", &[1, 96, 64], 0),       // 3: quantized
        tensor("int16", &[1, 1, 96, 64], 0),    // 4: delegate result
        tensor("float32", &[1, 1, 96, 64], 0),  // 5: dequantized
        tensor("float32", &[1, 1, 97, 65], 0),  // 6: padded
        tensor("float32", &[1, 32, 48, 32], 0), // 7: convolution result
        tensor("float32", &[1, 32, 48, 32], 0), // 8: method output
        scalar("double", "0.000244140625"),     // 9
        scalar("int", "0"),                     // 10
        scalar("int", "-32767"),                // 11
        scalar("int", "32767"),                 // 12
        scalar("int", "2"),                     // 13: int16
        scalar("None", ""),                     // 14
        scalar("int[]", "[0, 1, 0, 1]"),        // 15
        scalar("int[]", "[2, 2]"),              // 16
        scalar("int[]", "[0, 0]"),              // 17
        scalar("int[]", "[1, 1]"),              // 18
        scalar("bool", "false"),                // 19
        scalar("int", "1"),                     // 20
    ];
    for (i, value) in values.iter_mut().enumerate() {
        if let Some(t) = &mut value.tensor
            && t.data_buffer == 0
        {
            t.allocation = Some((
                1,
                match i {
                    3 | 5 => 24576,
                    6 => 49152,
                    _ => 0,
                },
            ));
        }
    }
    let calls = [
        (
            "quantized_decomposed::quantize_per_tensor.out",
            vec![0, 9, 10, 11, 12, 13, 3, 3],
            None,
        ),
        ("EthosUBackend", vec![3, 4], Some(0)),
        (
            "quantized_decomposed::dequantize_per_tensor.out",
            vec![4, 9, 10, 11, 12, 13, 14, 5, 5],
            None,
        ),
        ("aten::constant_pad_nd.out", vec![5, 15, 10, 6, 6], None),
        (
            "aten::convolution.out",
            vec![6, 1, 2, 16, 17, 18, 19, 17, 20, 7, 7],
            None,
        ),
        ("aten::relu.out", vec![7, 8, 8], None),
    ];
    Method {
        name: "forward".into(),
        inputs: vec![0],
        outputs: vec![8],
        values,
        operators: calls
            .iter()
            .filter(|c| c.2.is_none())
            .map(|c| c.0.into())
            .collect(),
        instructions: calls
            .into_iter()
            .enumerate()
            .map(|(index, (name, args, delegate))| Instruction {
                chain: 0,
                index,
                name: name.into(),
                args,
                delegate,
                kind: if delegate.is_some() { 2 } else { 1 },
                target: None,
            })
            .collect(),
        delegates: vec![Delegate {
            id: "EthosUBackend".into(),
            location: "fixture".into(),
            size: 0,
            specs: vec![],
            vela: Some(pte_explorer::vela::Vela {
                blocks: vec![],
                scratch_size: None,
                config: None,
                instructions: vec![],
                warnings: vec![],
                inputs: vec![pte_explorer::vela::Io {
                    shape: vec![1, 96, 64],
                    element_bytes: 2,
                    offset: 0,
                    region: 1,
                }],
                outputs: vec![pte_explorer::vela::Io {
                    shape: vec![1, 1, 96, 64],
                    element_bytes: 2,
                    offset: 0,
                    region: 1,
                }],
            }),
            warning: None,
        }],
        memory_sizes: vec![0, 262144],
        input_structure: String::new(),
        output_structure: String::new(),
    }
}

use crate::{
    flat::{Reader, Table},
    vela::Vela,
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};

#[derive(Debug, Serialize)]
pub struct Program {
    pub path: String,
    pub file_size: usize,
    pub version: u32,
    pub segment_base: u64,
    pub segments: Vec<Segment>,
    pub named_data: BTreeMap<String, u32>,
    pub methods: Vec<Method>,
}
#[derive(Debug, Serialize)]
pub struct Segment {
    pub offset: u64,
    pub size: u64,
}
#[derive(Debug, Serialize)]
pub struct Method {
    pub name: String,
    pub inputs: Vec<i32>,
    pub outputs: Vec<i32>,
    pub values: Vec<Value>,
    pub operators: Vec<String>,
    pub instructions: Vec<Instruction>,
    pub delegates: Vec<Delegate>,
    pub memory_sizes: Vec<i64>,
    pub input_structure: String,
    pub output_structure: String,
}
#[derive(Debug, Serialize)]
pub struct Value {
    pub kind: String,
    pub description: String,
    pub tensor: Option<Tensor>,
    pub references: Vec<i32>,
}
#[derive(Debug, Serialize)]
pub struct Tensor {
    pub dtype: String,
    pub shape: Vec<i32>,
    pub dim_order: Vec<u8>,
    pub storage_offset: u32,
    pub data_buffer: u32,
    pub allocation: Option<(u32, u64)>,
    pub shape_dynamism: u8,
    pub name: String,
}
#[derive(Debug, Serialize)]
pub struct Instruction {
    pub chain: usize,
    pub index: usize,
    pub name: String,
    pub args: Vec<i32>,
    pub delegate: Option<usize>,
    pub kind: u8,
    pub target: Option<i32>,
}
#[derive(Debug, Serialize)]
pub struct CompileSpec {
    pub key: String,
    pub text: Option<String>,
    pub hex: String,
}
#[derive(Debug, Serialize)]
pub struct Delegate {
    pub id: String,
    pub location: String,
    pub size: usize,
    pub specs: Vec<CompileSpec>,
    pub vela: Option<Vela>,
    pub warning: Option<String>,
}

fn dtype(x: u8) -> String {
    match x {
        0 => "uint8",
        1 => "int8",
        2 => "int16",
        3 => "int32",
        4 => "int64",
        5 => "float16",
        6 => "float32",
        7 => "float64",
        11 => "bool",
        12 => "qint8",
        13 => "quint8",
        14 => "qint32",
        15 => "bfloat16",
        16 => "quint4x2",
        17 => "quint2x4",
        22 => "bits16",
        23 => "float8e5m2",
        24 => "float8e4m3fn",
        25 => "float8e5m2fnuz",
        26 => "float8e4m3fnuz",
        27 => "uint16",
        28 => "uint32",
        29 => "uint64",
        _ => return format!("scalar_type({x})"),
    }
    .into()
}
fn value(t: Table<'_>) -> Result<Value> {
    let tag = t.u8(0)?;
    let v = t.child(1)?;
    let mut out = Value {
        kind: format!("type {tag}"),
        description: String::new(),
        tensor: None,
        references: vec![],
    };
    if tag == 0 || tag == 1 {
        out.kind = "None".into();
        return Ok(out);
    }
    let v = v.context("missing value union")?;
    let (kind, desc) = match tag {
        2 => ("int", (v.u64(0)? as i64).to_string()),
        3 => ("bool", (v.u8(0)? != 0).to_string()),
        4 => ("double", f64::from_bits(v.u64(0)?).to_string()),
        5 => {
            let a = v
                .child(6)?
                .map(|a| {
                    Ok::<_, anyhow::Error>((
                        a.u32(0)?,
                        a.u32(1)? as u64 | ((a.u32(2)? as u64) << 32),
                    ))
                })
                .transpose()?;
            let tensor = Tensor {
                dtype: dtype(v.u8(0)?),
                storage_offset: v.u32(1)?,
                shape: v.ints(2)?,
                dim_order: v.data(3)?.to_vec(),
                data_buffer: v.u32(5)?,
                allocation: a,
                shape_dynamism: v.u8(8)?,
                name: v
                    .child(9)?
                    .map(|x| x.string(1))
                    .transpose()?
                    .unwrap_or_default(),
            };
            let desc = format!("{} {:?}", tensor.dtype, tensor.shape);
            out.tensor = Some(tensor);
            ("Tensor", desc)
        }
        6 => ("string", v.string(0)?),
        7 => ("int[]", format!("{:?}", v.longs(0)?)),
        8 => {
            let (p, n) = v.vector(0, 8)?;
            let xs = (0..n)
                .map(|i| Ok(f64::from_bits(v.r.u64(p + 8 * i)?)))
                .collect::<Result<Vec<_>>>()?;
            ("double[]", format!("{xs:?}"))
        }
        9 => ("bool[]", format!("{:?}", v.data(0)?)),
        10 | 11 => {
            out.references = v.ints(0)?;
            (
                if tag == 10 { "Tensor[]" } else { "Tensor?[]" },
                format!("values {:?}", out.references),
            )
        }
        _ => ("unknown", format!("unsupported union {tag}")),
    };
    out.kind = kind.into();
    out.description = desc;
    Ok(out)
}
fn segment_data<'a>(
    data: &'a [u8],
    base: u64,
    segments: &[Segment],
    index: u32,
) -> Result<&'a [u8]> {
    let s = segments
        .get(index as usize)
        .context("segment index out of range")?;
    let start = usize::try_from(base.checked_add(s.offset).context("segment overflow")?)?;
    Reader(data).bytes(start, usize::try_from(s.size)?)
}
impl Program {
    pub fn open(path: &Path) -> Result<Self> {
        let bytes =
            std::fs::read(path).with_context(|| format!("Cannot read {}", path.display()))?;
        Self::parse(&bytes, path.to_string_lossy().as_ref())
    }
    pub fn parse(data: &[u8], path: &str) -> Result<Self> {
        let r = Reader(data);
        ensure!(
            r.bytes(4, 4)? == b"ET12",
            "Not a supported ExecuTorch ET12 program"
        );
        let root = r.root()?;
        let segment_base = if data.get(8..12) == Some(b"eh00") {
            ensure!(r.u32(12)? >= 24, "invalid extended header");
            r.bytes(8, r.u32(12)? as usize)?;
            r.u64(24)?
        } else {
            0
        };
        let segments = root
            .tables(4)?
            .iter()
            .map(|s| {
                Ok(Segment {
                    offset: s.u64(0)?,
                    size: s.u64(1)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        for i in 0..segments.len() {
            ensure!(
                segment_base > 0 || segments[i].size == 0,
                "segments require an extended header with a segment base"
            );
            segment_data(data, segment_base, &segments, i as u32)?;
        }
        let named_data = root
            .tables(7)?
            .iter()
            .map(|n| Ok((n.string(0)?, n.u32(1)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let mut methods = vec![];
        for m in root.tables(1)? {
            let values = m
                .tables(2)?
                .into_iter()
                .map(value)
                .collect::<Result<Vec<_>>>()?;
            let operators = m
                .tables(6)?
                .iter()
                .map(|op| {
                    let name = op.string(0)?;
                    let overload = op.string(1)?;
                    Ok(if overload.is_empty() {
                        name
                    } else {
                        format!("{name}.{overload}")
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let mut delegates = vec![];
            for d in m.tables(7)? {
                let id = d.string(0)?;
                let processed = d.required(1)?;
                let index = processed.u32(1)?;
                let (location, blob) = match processed.u8(0)? {
                    0 => (
                        format!("inline #{index}"),
                        root.tables(3)?
                            .get(index as usize)
                            .context("delegate inline index out of range")?
                            .data(0)?,
                    ),
                    1 => (
                        format!("segment #{index}"),
                        segment_data(data, segment_base, &segments, index)?,
                    ),
                    n => anyhow::bail!("unknown delegate location {n}"),
                };
                let specs = d
                    .tables(2)?
                    .iter()
                    .map(|s| {
                        let b = s.data(1)?;
                        Ok(CompileSpec {
                            key: s.string(0)?,
                            text: std::str::from_utf8(b)
                                .ok()
                                .filter(|s| {
                                    s.chars().all(|c| !c.is_control() || c == '\n' || c == '\t')
                                })
                                .map(str::to_owned),
                            hex: b.iter().map(|x| format!("{x:02x}")).collect(),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let mut delegate = Delegate {
                    id,
                    location,
                    size: blob.len(),
                    specs,
                    vela: None,
                    warning: None,
                };
                if delegate.id.to_lowercase().contains("ethos")
                    || blob.starts_with(b"vela_bin_stream")
                {
                    let external = |key: &str| -> Result<&[u8]> {
                        segment_data(
                            data,
                            segment_base,
                            &segments,
                            *named_data
                                .get(key)
                                .context("external Vela block absent from named_data")?,
                        )
                    };
                    match Vela::parse(blob, external) {
                        Ok(v) => delegate.vela = Some(v),
                        Err(e) => delegate.warning = Some(format!("Vela decode: {e:#}")),
                    }
                }
                delegates.push(delegate);
            }
            let mut instructions = vec![];
            for (chain, c) in m.tables(5)?.into_iter().enumerate() {
                for (index, i) in c.tables(2)?.into_iter().enumerate() {
                    let kind = i.u8(0)?;
                    let a = i.required(1)?;
                    let (name, args, delegate, target) = match kind {
                        1 => {
                            let op = a.u32(0)? as usize;
                            (
                                operators
                                    .get(op)
                                    .context("operator index out of range")?
                                    .clone(),
                                a.ints(1)?,
                                None,
                                None,
                            )
                        }
                        2 => {
                            let d = a.u32(0)? as usize;
                            (
                                delegates
                                    .get(d)
                                    .context("delegate index out of range")?
                                    .id
                                    .clone(),
                                a.ints(1)?,
                                Some(d),
                                None,
                            )
                        }
                        3 => (
                            "Move".into(),
                            vec![a.u32(0)? as i32, a.u32(1)? as i32],
                            None,
                            None,
                        ),
                        4 => (
                            "JumpFalse".into(),
                            vec![a.u32(0)? as i32],
                            None,
                            Some(a.u32(1)? as i32),
                        ),
                        5 => ("Free".into(), vec![a.u32(0)? as i32], None, None),
                        n => (format!("Unknown instruction {n}"), vec![], None, None),
                    };
                    for &arg in &args {
                        ensure!(
                            arg >= 0 && (arg as usize) < values.len(),
                            "invalid instruction argument {arg}"
                        );
                    }
                    instructions.push(Instruction {
                        chain,
                        index,
                        name,
                        args,
                        delegate,
                        kind,
                        target,
                    });
                }
            }
            let meta = m.child(1)?;
            methods.push(Method {
                name: m.string(0)?,
                inputs: m.ints(3)?,
                outputs: m.ints(4)?,
                values,
                operators,
                instructions,
                delegates,
                memory_sizes: m.longs(8)?,
                input_structure: meta.map(|x| x.string(0)).transpose()?.unwrap_or_default(),
                output_structure: meta.map(|x| x.string(1)).transpose()?.unwrap_or_default(),
            });
        }
        Ok(Self {
            path: path.into(),
            file_size: data.len(),
            version: root.u32(0)?,
            segment_base,
            segments,
            named_data,
            methods,
        })
    }
}

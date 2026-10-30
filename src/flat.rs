//! Small checked FlatBuffer reader. Field numbers follow ExecuTorch's ET12 schema.
use anyhow::{Result, bail, ensure};

#[derive(Clone, Copy)]
pub struct Reader<'a>(pub &'a [u8]);
#[derive(Clone, Copy)]
pub struct Table<'a> {
    pub r: Reader<'a>,
    pub pos: usize,
}
impl<'a> Reader<'a> {
    pub fn bytes(self, pos: usize, n: usize) -> Result<&'a [u8]> {
        self.0
            .get(
                pos..pos
                    .checked_add(n)
                    .ok_or_else(|| anyhow::anyhow!("offset overflow"))?,
            )
            .ok_or_else(|| anyhow::anyhow!("truncated data at 0x{pos:x}, need {n} bytes"))
    }
    pub fn u8(self, p: usize) -> Result<u8> {
        Ok(self.bytes(p, 1)?[0])
    }
    pub fn u16(self, p: usize) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes(p, 2)?.try_into()?))
    }
    pub fn u32(self, p: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(p, 4)?.try_into()?))
    }
    pub fn u64(self, p: usize) -> Result<u64> {
        Ok(u64::from_le_bytes(self.bytes(p, 8)?.try_into()?))
    }
    pub fn indirect(self, p: usize) -> Result<usize> {
        p.checked_add(self.u32(p)? as usize)
            .ok_or_else(|| anyhow::anyhow!("offset overflow"))
    }
    pub fn table(self, pos: usize) -> Result<Table<'a>> {
        let t = Table { r: self, pos };
        t.field(0)?;
        Ok(t)
    }
    pub fn root(self) -> Result<Table<'a>> {
        self.table(self.u32(0)? as usize)
    }
}
impl<'a> Table<'a> {
    pub fn field(self, field: usize) -> Result<Option<usize>> {
        let delta = self.r.u32(self.pos)? as i32 as i64;
        let v = (self.pos as i64)
            .checked_sub(delta)
            .filter(|p| *p >= 0)
            .ok_or_else(|| anyhow::anyhow!("invalid vtable"))? as usize;
        let len = self.r.u16(v)? as usize;
        ensure!(len >= 4 && len.is_multiple_of(2), "invalid vtable length");
        self.r.bytes(v, len)?;
        let size = self.r.u16(v + 2)? as usize;
        self.r.bytes(self.pos, size)?;
        if 4 + field * 2 >= len {
            return Ok(None);
        }
        let off = self.r.u16(v + 4 + field * 2)? as usize;
        if off == 0 {
            return Ok(None);
        }
        ensure!(off >= 4 && off < size, "invalid field offset");
        Ok(Some(self.pos + off))
    }
    pub fn u8(self, f: usize) -> Result<u8> {
        self.field(f)?.map_or(Ok(0), |p| self.r.u8(p))
    }
    pub fn u32(self, f: usize) -> Result<u32> {
        self.field(f)?.map_or(Ok(0), |p| self.r.u32(p))
    }
    pub fn u64(self, f: usize) -> Result<u64> {
        self.field(f)?.map_or(Ok(0), |p| self.r.u64(p))
    }
    pub fn child(self, f: usize) -> Result<Option<Self>> {
        self.field(f)?
            .map(|p| self.r.table(self.r.indirect(p)?))
            .transpose()
    }
    pub fn required(self, f: usize) -> Result<Self> {
        self.child(f)?
            .ok_or_else(|| anyhow::anyhow!("missing table field {f}"))
    }
    pub fn vector(self, f: usize, width: usize) -> Result<(usize, usize)> {
        let Some(p) = self.field(f)? else {
            return Ok((0, 0));
        };
        let v = self.r.indirect(p)?;
        let n = self.r.u32(v)? as usize;
        ensure!(n <= 2_000_000, "vector exceeds safety limit");
        self.r.bytes(
            v + 4,
            n.checked_mul(width)
                .ok_or_else(|| anyhow::anyhow!("vector overflow"))?,
        )?;
        Ok((v + 4, n))
    }
    pub fn tables(self, f: usize) -> Result<Vec<Self>> {
        let (p, n) = self.vector(f, 4)?;
        (0..n)
            .map(|i| self.r.table(self.r.indirect(p + i * 4)?))
            .collect()
    }
    pub fn data(self, f: usize) -> Result<&'a [u8]> {
        let (p, n) = self.vector(f, 1)?;
        self.r.bytes(p, n)
    }
    pub fn string(self, f: usize) -> Result<String> {
        let data = self.data(f)?;
        if data.len() > 1_000_000 {
            bail!("string too long");
        }
        Ok(std::str::from_utf8(data)?.to_owned())
    }
    pub fn ints(self, f: usize) -> Result<Vec<i32>> {
        let (p, n) = self.vector(f, 4)?;
        (0..n).map(|i| Ok(self.r.u32(p + 4 * i)? as i32)).collect()
    }
    pub fn longs(self, f: usize) -> Result<Vec<i64>> {
        let (p, n) = self.vector(f, 8)?;
        (0..n).map(|i| Ok(self.r.u64(p + 8 * i)? as i64)).collect()
    }
}

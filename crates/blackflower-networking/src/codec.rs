use bytes::{BufMut as _, Bytes, BytesMut};

use crate::wire::{WireError, copy_array};

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    pub(crate) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    pub(crate) fn u8(&mut self) -> Result<u8, WireError> {
        let value = *self.bytes.get(self.cursor).ok_or(WireError::Truncated)?;
        self.cursor += 1;
        Ok(value)
    }

    pub(crate) fn u16(&mut self) -> Result<u16, WireError> {
        Ok(u16::from_le_bytes(copy_array(self.take(2)?)?))
    }

    pub(crate) fn u32(&mut self) -> Result<u32, WireError> {
        Ok(u32::from_le_bytes(copy_array(self.take(4)?)?))
    }

    pub(crate) fn u64(&mut self) -> Result<u64, WireError> {
        Ok(u64::from_le_bytes(copy_array(self.take(8)?)?))
    }

    pub(crate) fn fixed<const N: usize>(&mut self) -> Result<[u8; N], WireError> {
        copy_array(self.take(N)?)
    }

    pub(crate) fn bytes_u16(&mut self, maximum: usize) -> Result<Bytes, WireError> {
        let length = usize::from(self.u16()?);
        if length > maximum {
            return Err(WireError::Oversized {
                actual: length,
                maximum,
            });
        }
        Ok(Bytes::copy_from_slice(self.take(length)?))
    }

    pub(crate) fn remainder(&mut self, maximum: usize) -> Result<&'a [u8], WireError> {
        let bytes = self.take(self.remaining())?;
        if bytes.len() > maximum {
            return Err(WireError::Oversized {
                actual: bytes.len(),
                maximum,
            });
        }
        Ok(bytes)
    }

    pub(crate) fn finish(self) -> Result<(), WireError> {
        if self.cursor == self.bytes.len() {
            Ok(())
        } else {
            Err(WireError::Trailing)
        }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], WireError> {
        let end = self
            .cursor
            .checked_add(length)
            .ok_or(WireError::IntegerOutOfRange)?;
        let bytes = self
            .bytes
            .get(self.cursor..end)
            .ok_or(WireError::Truncated)?;
        self.cursor = end;
        Ok(bytes)
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.cursor)
    }
}

pub(crate) struct Writer {
    bytes: BytesMut,
}

impl Writer {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes: BytesMut::with_capacity(capacity),
        }
    }

    pub(crate) fn u8(&mut self, value: u8) {
        self.bytes.put_u8(value);
    }

    pub(crate) fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(crate) fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(crate) fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(crate) fn fixed(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }

    pub(crate) fn bytes_u16(&mut self, value: &[u8]) -> Result<(), WireError> {
        let length = u16::try_from(value.len()).map_err(|_error| WireError::IntegerOutOfRange)?;
        self.u16(length);
        self.fixed(value);
        Ok(())
    }

    pub(crate) fn finish(self) -> Bytes {
        self.bytes.freeze()
    }
}

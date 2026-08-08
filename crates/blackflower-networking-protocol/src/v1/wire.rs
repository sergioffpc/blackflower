use bytes::BytesMut;

use super::ProtocolError;

pub(crate) fn ensure_length(
    bytes: &[u8],
    expected: usize,
    schema: &'static str,
) -> Result<(), ProtocolError> {
    if bytes.len() == expected {
        Ok(())
    } else {
        Err(ProtocolError::InvalidLength {
            schema,
            expected,
            actual: bytes.len(),
        })
    }
}

pub(crate) struct Decoder<'a> {
    remaining: &'a [u8],
    expected: usize,
    schema: &'static str,
}

impl<'a> Decoder<'a> {
    pub(crate) const fn new(bytes: &'a [u8], schema: &'static str) -> Self {
        Self {
            remaining: bytes,
            expected: bytes.len(),
            schema,
        }
    }

    pub(crate) fn u8(&mut self) -> Result<u8, ProtocolError> {
        Ok(self.take::<1>()?[0])
    }

    pub(crate) fn i16(&mut self) -> Result<i16, ProtocolError> {
        Ok(i16::from_le_bytes(self.take()?))
    }

    pub(crate) fn u16(&mut self) -> Result<u16, ProtocolError> {
        Ok(u16::from_le_bytes(self.take()?))
    }

    pub(crate) fn i32(&mut self) -> Result<i32, ProtocolError> {
        Ok(i32::from_le_bytes(self.take()?))
    }

    pub(crate) fn u64(&mut self) -> Result<u64, ProtocolError> {
        Ok(u64::from_le_bytes(self.take()?))
    }

    pub(crate) fn finish(self) -> Result<(), ProtocolError> {
        if self.remaining.is_empty() {
            Ok(())
        } else {
            Err(ProtocolError::InvalidLength {
                schema: self.schema,
                expected: self.expected.saturating_sub(self.remaining.len()),
                actual: self.expected,
            })
        }
    }

    fn take<const SIZE: usize>(&mut self) -> Result<[u8; SIZE], ProtocolError> {
        let Some((head, tail)) = self.remaining.split_at_checked(SIZE) else {
            return Err(ProtocolError::InvalidLength {
                schema: self.schema,
                expected: self.expected,
                actual: self.expected.saturating_sub(self.remaining.len()),
            });
        };
        let mut value = [0_u8; SIZE];
        value.copy_from_slice(head);
        self.remaining = tail;
        Ok(value)
    }
}

pub(crate) fn put_i16(output: &mut BytesMut, value: i16) {
    output.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn put_i32(output: &mut BytesMut, value: i32) {
    output.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn put_u64(output: &mut BytesMut, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

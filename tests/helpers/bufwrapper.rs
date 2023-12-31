use std::ops::Deref;

#[derive(Clone, Debug)]
pub struct BufWrapper {
    inner: Box<[u8]>,
    len: usize,
}

impl BufWrapper {
    pub fn new() -> Self {
        Self {
            inner: Box::new([0u8; 1518]),
            len: 0,
        }
    }

    pub fn exact_slice(&self) -> &[u8] {
        &self.inner[0..self.len]
    }

    pub fn as_ref(&self) -> &[u8] {
        &self.inner.as_ref()
    }

    pub fn as_mut_ref(&mut self) -> &mut [u8] {
        self.inner.as_mut()
    }

    pub fn write(&mut self, byte: u8, count: usize) {
        for i in 0..count {
            self.inner[i] = byte;
        }
        self.len = count;
    }
}

impl Deref for BufWrapper {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

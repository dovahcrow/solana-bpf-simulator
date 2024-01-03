pub trait AccountSizes {
    fn len(&self) -> usize;
    fn size(&self, i: usize) -> usize;
}

impl AccountSizes for Vec<usize> {
    fn len(&self) -> usize {
        self.len()
    }
    fn size(&self, i: usize) -> usize {
        self[i]
    }
}

impl AccountSizes for (usize, usize) {
    fn len(&self) -> usize {
        self.0
    }
    fn size(&self, _: usize) -> usize {
        self.1
    }
}

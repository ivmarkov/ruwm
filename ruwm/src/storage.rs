pub trait Storage<D> {
    fn get(&self) -> D;
    fn set(&mut self, data: D);
}

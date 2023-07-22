pub trait Separable {
    fn is_separator(&self) -> bool;
}

impl Separable for char {
    fn is_separator(&self) -> bool {
        *self == ' ' || *self == '-' || *self == ',' || *self == ':' || *self == '.'
    }
}
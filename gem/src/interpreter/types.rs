use crate::interpreter::Value;
use std::cmp::Ordering;
use std::collections::HashMap;

pub trait Indexable<T> {
    fn index<'a>(&'a self, index: usize) -> Result<&'a T, String>;

    fn index_mut<'a>(&'a mut self, index: usize) -> Result<&'a mut T, String>;
}

pub trait Valuable {
    fn inner(&self) -> &Value;
    fn set_value(&mut self, val: Value);
}

pub trait Object {
    fn get_prop(&self, prop: &'static str) -> Option<&dyn Valuable>;
    fn set_prop(&mut self, prop: &'static str, val: Box<dyn Valuable>);
}
#[derive(Debug, Clone, PartialEq)]
pub struct EmObject {
    pub members: HashMap<&'static str, Box<Value>>,
}

impl EmObject {
    pub fn get_prop(&self, prop: &'static str) -> Option<&Value> {
        if let Some(val) = self.members.get(prop) {
            Some(&**val)
        } else {
            None
        }
    }

    pub fn set_prop(&mut self, prop: &'static str, val: Box<Value>) {
        self.members.insert(prop, val);
    }
}

//This is probably really bad but idk how else to compare them
impl PartialOrd for EmObject {
    fn partial_cmp(&self, other: &EmObject) -> Option<Ordering> {
        Some(self.members.len().cmp(&other.members.len()))
    }
}

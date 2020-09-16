use super::{ Data, View };
use std::cell::RefCell;
use std::rc::Rc;

type ViewPtr = Rc<RefCell<View>>;
type DataPtr = Rc<RefCell<Data>>;

struct Controller {
    view: ViewPtr,
    data: DataPtr,
}

impl Controller {
    pub fn new(view: &ViewPtr, data: &DataPtr) -> Self {
        Self {
            view: view.clone(),
            data: data.clone(),
        }
    }
}

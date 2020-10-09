//! Represents an integer in Jinko. All integers are signed 64 bytes

use super::Value;
use crate::instruction::{InstrKind, Instruction};
use crate::{Interpreter, JinkoError};

#[derive(Clone)]
pub struct JinkInt(i64);

impl From<i64> for JinkInt {
    fn from(i: i64) -> Self {
        JinkInt(i)
    }
}

impl Value for JinkInt {}

impl Instruction for JinkInt {
    fn kind(&self) -> InstrKind {
        InstrKind::Expression
    }

    fn print(&self) -> String {
        self.0.to_string()
    }

    fn execute(&self, interpreter: &mut Interpreter) -> Result<(), JinkoError> {
        // FIXME: Add logic
        interpreter.debug("INT", &self.0.to_string());

        Ok(())
    }
}

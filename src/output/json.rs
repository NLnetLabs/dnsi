use crate::client::Answer;
use std::io;

pub fn write(answer: &Answer, target: &mut impl io::Write) -> Result<(), io::Error> {
    serde_json::to_writer_pretty(target, answer.message()).unwrap();
    Ok(())
}

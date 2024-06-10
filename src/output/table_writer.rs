use std::io;

use super::ansi::{ITALIC, RESET, UNDERLINE};

pub struct TableWriter<'a, const N: usize> {
    pub indent: &'a str,
    pub spacing: &'a str,
    pub header: Option<[&'a str; N]>,
    pub rows: &'a [[String; N]],
    pub enabled_columns: [bool; N],
    pub right_aligned: [bool; N],
}

impl<const N: usize> Default for TableWriter<'_, N> {
    fn default() -> Self {
        Self {
            indent: "",
            spacing: "  ",
            header: None,
            rows: &[],
            enabled_columns: [true; N],
            right_aligned: [false; N],
        }
    }
}

impl<const N: usize> TableWriter<'_, N> {
    pub fn write(&self, mut target: impl io::Write) -> io::Result<()> {
        let Self {
            indent,
            spacing,
            header,
            rows,
            enabled_columns,
            right_aligned,
        } = self;

        let mut widths = [0; N];

        if let Some(header) = header {
            for i in 0..N {
                widths[i] = header[i].len();
            }
        }

        for row in *rows {
            for i in 0..N {
                widths[i] = widths[i].max(row[i].len());
            }
        }

        let columns: Vec<_> = (0..N).filter(|i| enabled_columns[*i]).collect();

        if columns.is_empty() {
            return Ok(());
        }

        if let Some(header) = self.header {
            write!(target, "{indent}{UNDERLINE}{ITALIC}")?;
            for &i in &columns[..columns.len() - 1] {
                write!(target, "{:<width$}{spacing}", header[i], width = widths[i])?;
            }
            let last = columns[columns.len() - 1];
            write!(target, "{:<width$}", header[last], width = widths[last])?;
            writeln!(target, "{RESET}")?;
        }

        for row in *rows {
            write!(target, "{indent}")?;
            for &i in &columns[..columns.len() - 1] {
                if right_aligned[i] {
                    write!(target, "{:>width$}", row[i], width = widths[i])?;
                } else {
                    write!(target, "{:<width$}", row[i], width = widths[i])?;
                }
                write!(target, "{spacing}")?;
            }
            let last = columns[columns.len() - 1];
            if right_aligned[last] {
                write!(target, "{:>width$}", row[last], width = widths[last])?;
            } else {
                write!(target, "{:<width$}", row[last], width = widths[last])?;
            }
            writeln!(target)?;
        }

        Ok(())
    }
}

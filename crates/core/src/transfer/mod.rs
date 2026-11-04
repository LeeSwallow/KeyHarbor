//! Moving items in and out of the vault as bytes; the CLI reads and writes the files.
//! [`backup`] is KeyHarbor's own encrypted export in the age format (07 §7.7), [`chrome`]
//! reads Chrome's password export, [`dotenv`] a `.env` file. Error messages name lines and
//! columns, never values.

mod backup;
mod chrome;
mod dotenv;

pub use backup::ImportReport;
pub use dotenv::parse_dotenv;

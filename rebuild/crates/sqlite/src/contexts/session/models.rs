//! Rows of the session context as Diesel reads and writes them.

use super::schema::{generations, handoff_requests, sessions};
use diesel::prelude::*;

#[derive(Insertable)]
#[diesel(table_name = sessions)]
pub struct NewSession<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub state: &'a str,
}

/// A session joined with the generation of its current run.
#[derive(Queryable)]
pub struct SessionModel {
    pub id: String,
    pub name: String,
    pub state: String,
    pub generation: Option<String>,
    pub cdp: Option<bool>,
}

#[derive(Insertable)]
#[diesel(table_name = generations)]
pub struct NewGeneration<'a> {
    pub session_id: &'a str,
    pub generation: &'a str,
    pub cdp: bool,
}

#[derive(Insertable)]
#[diesel(table_name = handoff_requests)]
pub struct NewHandoff<'a> {
    pub request_id: &'a str,
    pub session_id: &'a str,
    pub generation: &'a str,
    pub origin: Option<&'a str>,
    pub status: &'a str,
}

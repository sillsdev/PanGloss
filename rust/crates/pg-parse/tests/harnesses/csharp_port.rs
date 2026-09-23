// Included test files share support modules (`mod common;`), so each is loaded once per includer.
#![allow(clippy::duplicate_mod)]

#[path = "../csharp_port_affix_process.rs"]
mod csharp_port_affix_process;
#[path = "../csharp_port_affix_template.rs"]
mod csharp_port_affix_template;
#[path = "../csharp_port_compounding.rs"]
mod csharp_port_compounding;
#[path = "../csharp_port_generation.rs"]
mod csharp_port_generation;
#[path = "../csharp_port_lex_entry.rs"]
mod csharp_port_lex_entry;
#[path = "../csharp_port_metathesis.rs"]
mod csharp_port_metathesis;
#[path = "../csharp_port_morpher.rs"]
mod csharp_port_morpher;
#[path = "../csharp_port_rewrite.rs"]
mod csharp_port_rewrite;

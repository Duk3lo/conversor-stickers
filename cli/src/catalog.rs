use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use conversor_core::{CatalogError, generate_index, serialize_index, validate_catalog};
use thiserror::Error;

use crate::arguments::SpanishFormat;
use crate::codes;
use crate::writing::write_atomic;

#[derive(Debug, Parser)]
#[command(
    name = "conversor-stickers catalog",
    about = "Valida el catálogo de stickers de la comunidad o genera su index.json.",
    disable_version_flag = true,
    subcommand_required = true,
    arg_required_else_help = true
)]
struct CatalogArguments {
    #[command(subcommand)]
    command: CatalogCommand,
}

#[derive(Debug, Subcommand)]
enum CatalogCommand {
    #[command(about = "Comprueba las reglas del catálogo en todos los paquetes de DIR.")]
    Validate {
        #[arg(
            value_name = "DIR",
            help = "Directorio del catálogo: categories.json, withdrawn.json y packs/<id>/."
        )]
        directory: PathBuf,
    },
    #[command(
        about = "Valida el catálogo de DIR y escribe su index.json con las sumas SHA-256. El \
                 mismo catálogo da siempre los mismos bytes."
    )]
    Index {
        #[arg(value_name = "DIR", help = "Directorio del catálogo.")]
        directory: PathBuf,
        #[arg(
            long,
            value_name = "ARCHIVO",
            help = "Archivo del índice; se sustituye si ya existe."
        )]
        output: PathBuf,
    },
}

#[derive(Debug, Error)]
enum CommandError {
    #[error(transparent)]
    Catalog(#[from] CatalogError),

    #[error("no se pudo escribir {}: {source}", path.display())]
    Writing {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("no se pudo escribir en la salida estándar: {0}")]
    Stdout(#[source] io::Error),
}

impl CommandError {
    fn code(&self) -> u8 {
        match self {
            Self::Catalog(CatalogError::Reading { .. } | CatalogError::InvalidFileJson { .. }) => {
                codes::OTHER_ERROR
            }
            Self::Catalog(_) => codes::INVALID_INPUT,
            Self::Writing { .. } | Self::Stdout(_) => codes::OTHER_ERROR,
        }
    }
}

pub(crate) fn run(arguments: impl IntoIterator<Item = std::ffi::OsString>) -> u8 {
    let input =
        std::iter::once(std::ffi::OsString::from("conversor-stickers catalog")).chain(arguments);
    let arguments = match CatalogArguments::try_parse_from(input) {
        Ok(arguments) => arguments,
        Err(error) => {
            let code = if error.use_stderr() {
                codes::INVALID_ARGUMENTS
            } else {
                codes::SUCCESS
            };
            let _ = error.apply::<SpanishFormat>().print();
            return code;
        }
    };
    match run_command(&arguments.command) {
        Ok(()) => codes::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            error.code()
        }
    }
}

fn run_command(command: &CatalogCommand) -> Result<(), CommandError> {
    match command {
        CatalogCommand::Validate { directory } => {
            let catalog = validate_catalog(directory)?;
            write_stdout(&format!(
                "Catálogo válido: {} paquetes, {} categorías, {} retirados\n",
                catalog.packs.len(),
                catalog.categories.len(),
                catalog.withdrawn.len()
            ))
        }
        CatalogCommand::Index { directory, output } => write_index(directory, output),
    }
}

fn write_index(directory: &Path, output: &Path) -> Result<(), CommandError> {
    let index = generate_index(directory)?;
    let text = serialize_index(&index)?;
    write_atomic(output, text.as_bytes()).map_err(|source| CommandError::Writing {
        path: output.to_path_buf(),
        source,
    })?;
    write_stdout(&format!(
        "Índice escrito en {} ({} paquetes)\n",
        output.display(),
        index.packs.len()
    ))
}

fn write_stdout(text: &str) -> Result<(), CommandError> {
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
        .map_err(CommandError::Stdout)
}

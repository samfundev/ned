extern crate ansi_term;
extern crate getopts;
extern crate glob;
extern crate regex;
extern crate walkdir;

mod files;
mod opts;
mod parameters;
mod source;
#[cfg(test)]
mod tests;

use ansi_term::Colour::{Purple, Red};
use files::Files;
use opts::{make_opts, PROGRAM, usage_full, usage_version};
use parameters::{get_parameters, Parameters};
use source::Source;
use std::borrow::Cow;
use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::iter::Iterator;
use std::string::String;
use std::{env, process};

fn main() {

    let args = get_args();

    // Output is passed here so that tests can
    // call ned() directly to read the output
    // that would go to stdout.
    let mut output = io::stdout();
    match ned(&args, &mut output) {
        Ok(exit_code) => process::exit(exit_code),
        Err(err) => {
            println!("{}: {}", PROGRAM, err.to_string());
            process::exit(1)
        }
    }
}

fn get_args() -> Vec<String> {
    let mut args = env::args().skip(1).collect();
    if let Ok(default_args) = env::var("NED_DEFAULTS") {
        let old_args = args;
        args = default_args.split_whitespace().map(|s| s.to_string()).collect::<Vec<String>>();
        args.extend(old_args);
    }
    args
}

fn ned(args: &[String], mut output: &mut Write) -> Result<i32, String> {

    let opts = make_opts();
    let parameters = try!(get_parameters(&opts, args));

    if parameters.version {
        println!("{}", usage_version());
        process::exit(1);
    }

    if parameters.regex.is_none() || parameters.help {
        println!("{}", usage_full(&opts));
        process::exit(1);
    }

    let found_matches = try!(process_files(&parameters, &mut output));
    Ok(if found_matches {
        0
    } else {
        1
    })
}

fn process_files(parameters: &Parameters, output: &mut Write) -> Result<bool, String> {
    let mut found_matches = false;
    if parameters.stdin {
        let mut source = Source::Stdin(Box::new(io::stdin()));
        found_matches |= try!(process_file(&parameters, None, &mut source, output));
    } else {
        for glob in &parameters.globs {
            for path_buf in &mut Files::new(&parameters, &glob) {
                match OpenOptions::new()
                          .read(true)
                          .write(parameters.replace
                                           .is_some())
                          .open(path_buf.as_path()) {
                    Ok(file) => {
                        let mut source = Source::File(Box::new(file));
                        found_matches |= match process_file(&parameters,
                                                            Some(path_buf.as_path()
                                                                         .to_string_lossy()),
                                                            &mut source,
                                                            output) {
                            Ok(found_matches) => found_matches,
                            Err(err) => {
                                io::stderr()
                                    .write(&format!("{}: {} {}\n",
                                                    PROGRAM,
                                                    path_buf.as_path().to_string_lossy(),
                                                    err.to_string())
                                                .into_bytes())
                                    .expect("Can't write to stderr!");
                                false
                            }
                        }
                    }
                    Err(err) => {
                        io::stderr()
                            .write(&format!("{}: {} {}\n",
                                            PROGRAM,
                                            path_buf.as_path().to_string_lossy(),
                                            err.to_string())
                                        .into_bytes())
                            .expect("Can't write to stderr!");
                    }
                }
            }
        }
    }
    try!(output.flush().map_err(|err| err.to_string()));
    Ok(found_matches)
}

fn process_file(parameters: &Parameters,
                file_name: Option<Cow<str>>,
                source: &mut Source,
                mut output: &mut Write)
                -> Result<bool, String> {
    let purple = Purple;
    let red = Red.bold();

    let file_name: Option<Cow<str>> = if let Some(file_name) = file_name {
        let mut file_name = file_name.to_string();
        if parameters.colors {
            file_name = purple.paint(file_name).to_string();
        }
        file_name = if parameters.whole_files {
            format!("{}:\n", file_name)
        } else {
            format!("{}: ", file_name)
        };
        Some(Cow::Owned(file_name))
    } else {
        None
    };

    let content;
    {
        let read: &mut Read = match source {
            &mut Source::Stdin(ref mut read) => read,
            &mut Source::File(ref mut file) => file,
            #[cfg(test)]
            &mut Source::Cursor(ref mut cursor) => cursor,
        };
        let mut buffer = Vec::new();
        let _ = try!(read.read_to_end(&mut buffer).map_err(|err| err.to_string()));
        content = try!(String::from_utf8(buffer).map_err(|err| err.to_string()));
    }

    let re = parameters.regex.clone().expect("Bug, already checked parameters.");
    let mut found_matches = false;

    if let Some(mut replace) = parameters.replace.clone() {
        if parameters.colors {
            replace = red.paint(replace.as_str()).to_string();
        }
        let new_content = re.replace_all(&content, replace.as_str());
        // The replace has to do at least on allocation, so keep the old copy
        // to figure out if there where patches, to save unncessary regex match.
        found_matches = new_content != content;
        if parameters.stdout {
            if !parameters.quiet {
                if let Some(ref file_name) = file_name {
                    try!(output.write(&file_name.to_string()
                                                .into_bytes())
                               .map_err(|err| err.to_string()));
                }
                try!(output.write(&new_content.into_bytes()).map_err(|err| err.to_string()));
            }
        } else {
            match source {
                // A better way???
                &mut Source::File(ref mut file) => {
                    try!(file.seek(SeekFrom::Start(0)).map_err(|err| err.to_string()));
                    try!(file.write(&new_content.into_bytes()).map_err(|err| err.to_string()));
                }
                #[cfg(test)]
                &mut Source::Cursor(ref mut cursor) => {
                    try!(cursor.seek(SeekFrom::Start(0)).map_err(|err| err.to_string()));
                    try!(cursor.write(&new_content.into_bytes()).map_err(|err| err.to_string()));
                }
                _ => {}
            }
        }
    } else if parameters.quiet {
        // Quiet match only is shortcut by the more performant is_match() .
        found_matches = re.is_match(&content);
    } else {
        let mut process_text = |text: &str| -> Result<bool, String> {
            if let Some(ref group) = parameters.group {
                if let Some(captures) = re.captures(&text) {
                    let matched = match group.trim().parse::<usize>() {
                        Ok(index) => captures.at(index),
                        Err(_) => captures.name(group),
                    };
                    if let Some(matched) = matched {
                        let mut matched = matched.to_string();
                        if parameters.colors {
                            matched = re.replace_all(&matched,
                                                     red.paint("$0")
                                                        .to_string()
                                                        .as_str());
                        }
                        if let Some(ref file_name) = file_name {
                            try!(output.write(&file_name.to_string()
                                                        .into_bytes())
                                       .map_err(|err| err.to_string()));
                        }
                        try!(output.write(&matched.to_string().into_bytes())
                                   .map_err(|err| err.to_string()));
                        if !matched.ends_with("\n") {
                            try!(output.write(&"\n".to_string().into_bytes())
                                       .map_err(|err| err.to_string()));
                        }
                    }
                    return Ok(true);
                }
                return Ok(false);
            } else if parameters.no_match {
                let found_matches = re.is_match(&text);
                if !found_matches {
                    if let Some(ref file_name) = file_name {
                        try!(output.write(&file_name.to_string()
                                                    .into_bytes())
                                   .map_err(|err| err.to_string()));
                    }
                    try!(output.write(&text.to_string().into_bytes())
                               .map_err(|err| err.to_string()));
                    if !text.ends_with("\n") {
                        try!(output.write(&"\n".to_string().into_bytes())
                                   .map_err(|err| err.to_string()));
                    }
                }
                return Ok(found_matches);
            } else if re.is_match(&text) {
                if parameters.only_matches {
                    if let Some(ref file_name) = file_name {
                        try!(output.write(&file_name.to_string()
                                                    .into_bytes())
                                   .map_err(|err| err.to_string()));
                    }
                    for (start, end) in re.find_iter(&text) {
                        let mut matched = text[start..end].to_string();
                        if parameters.colors {
                            matched = re.replace_all(&matched,
                                                     red.paint("$0").to_string().as_str());
                        }
                        try!(output.write(&matched.to_string().into_bytes())
                                   .map_err(|err| err.to_string()));
                        if !matched.ends_with("\n") {
                            try!(output.write(&"\n".to_string().into_bytes())
                                       .map_err(|err| err.to_string()));
                        }
                    }
                } else {
                    if let Some(ref file_name) = file_name {
                        try!(output.write(&file_name.to_string()
                                                    .into_bytes())
                                   .map_err(|err| err.to_string()));
                    }
                    let mut text = text.to_string();
                    if parameters.colors {
                        text = re.replace_all(&text, red.paint("$0").to_string().as_str());
                    }
                    try!(output.write(&text.to_string().into_bytes())
                               .map_err(|err| err.to_string()));
                    if !text.ends_with("\n") {
                        try!(output.write(&"\n".to_string().into_bytes())
                                   .map_err(|err| err.to_string()));
                    }
                }
                return Ok(true);
            } else {
                return Ok(false);
            }
        };

        if !parameters.whole_files {
            for line in content.lines() {
                found_matches |= try!(process_text(&line));
            }
        } else {
            found_matches = try!(process_text(&content));
        }
    }
    Ok(found_matches)
}

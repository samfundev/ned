extern crate ansi_term;
extern crate getopts;
extern crate glob;
extern crate regex;
extern crate walkdir;

use ansi_term::Colour::Red;
use getopts::{Matches, Options, ParsingStyle};
use glob::Pattern;
use regex::Regex;
use std::fs::{File, OpenOptions};
#[cfg(test)]
use std::io::Cursor;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::string::String;
use std::{env, path, process};
use walkdir::{WalkDir, WalkDirIterator};

#[cfg(test)]
mod test_matches;
#[cfg(test)]
mod test_other;

enum Source {
    Stdin(Box<Read>),
    File(Box<File>),
    #[cfg(test)]
    Cursor(Box<Cursor<Vec<u8>>>),
}

fn main() {
    let (program, args) = get_program_and_args();

    match ned(&program, &args) {
        Ok(exit_code) => process::exit(exit_code),
        Err(err) => {
            println!("{}: {}", &program, err.to_string());
            process::exit(1)
        }
    }
}

fn ned(program: &str, args: &Vec<String>) -> Result<i32, String> {
    let opts = make_opts();
    let parsed = opts.parse(args);
    if let Err(err) = parsed {
        println!("{}: {}", &program, err.to_string());
        process::exit(1);
    }

    let matches = parsed.expect("Bug, already checked for a getopts parse error.");
    if matches.opt_present("version") {
        println!("{}{}", &VERSION, &LICENSE);
        process::exit(1);
    }

    if matches.free.len() == 0 && !matches.opt_present("pattern") || matches.opt_present("h") {
        let brief = format!("Usage: {} {}\n{}",
                            program,
                            &OPTS_AND_ARGS,
                            &PRE_DESCRIPTION);
        println!("\n{}{}{}{}",
                 opts.usage(&brief),
                 &POST_DESCRIPTION,
                 &VERSION,
                 &LICENSE);
        process::exit(1);
    }

    let (pattern, file_names) = match matches.opt_str("pattern") {
        Some(pattern) => (pattern.clone(), matches.free.iter().collect::<Vec<_>>()),
        None => (matches.free[0].clone(), matches.free.iter().skip(1).collect::<Vec<_>>()),
    };

    let pattern = add_re_options_to_pattern(&matches, &pattern);

    let re = Regex::new(&pattern);
    if let Err(err) = re {
        println!("{}: {}", &program, err.to_string());
        process::exit(1);
    }

    //    let files = try!(get_files(&matches, &file_names, |path| {
    // let file = try!(OpenOptions::new()
    // .read(true)
    // .write(matches.opt_present("replace"))
    // .open(path)
    // .map_err(|e| e.to_string()));
    // Ok(Source::File(Box::new(file)))
    // }).map_err(|e| e.to_string()));
    // Output is passed here so that tests can read the output.
    // / *    let mut output = io::stdout();
    // let exit_status = try!(process_files(&matches,
    // re.expect("Bug, already checked for a regex parse \
    // error."),
    // &mut files,
    // &mut output)
    // .map_err(|e| e.to_string()));
    // Ok(exit_status)
    Ok(0)
}

static OPTS_AND_ARGS: &'static str = "[OPTION]... [-p] <PATTERN> [FILE]...";
static PRE_DESCRIPTION: &'static str = "
ned is a bit like grep and a bit like sed, but not really. FILEs are ascii or
utf-8 text files.

For regex syntax see: http://rust-lang-nursery.github.io/regex/regex/#syntax";
static POST_DESCRIPTION: &'static str = "
Environment:
    NED_DEFAULTS        ned options prepended to the program's arguments

Exit codes:
    0                   matches found/replaced
    1                   no matches

Quiet:
    When -q --quiet is  specified ned tests for matches and returns an exit
    code of 0 if a match is found in ANY file. When -a --all is combined with
    quiet it returns an exit code of 0 if a match is found in ALL files. Quiet
    any matches will only read only as many files as needed to find a match.
    Quiet matches are more performant than non-quiet matches.
";
static VERSION: &'static str = "
ned 0.1.0 Copyright (C) 2016 Nev Delap - https://github.com/nevdelap/ned
";
static LICENSE: &'static str = "
License GPLv3+: GNU GPL version 3 or later <http://gnu.org/licenses/gpl.html>.
This is free software: you are free to change and redistribute it.
There is NO WARRANTY, to the extent permitted by law.
";

fn get_program_and_args() -> (String, Vec<String>) {
    let args: Vec<String> = env::args().collect();
    let program = path::Path::new(&args[0])
                      .file_name()
                      .expect("Bug, could't get bin name.")
                      .to_str()
                      .expect("Bug, could't get bin name.");
    let mut args: Vec<String> = args.iter().skip(1).map(|arg| arg.clone()).collect();
    if let Ok(default_args) = env::var("NED_DEFAULTS") {
        let old_args = args;
        args = default_args.split_whitespace().map(|s| s.to_string()).collect::<Vec<String>>();
        args.extend(old_args);;
    }
    (program.to_string(), args)
}

fn make_opts() -> Options {
    let mut opts = Options::new();
    opts.parsing_style(ParsingStyle::FloatingFrees);
    opts.optopt("p",
                "pattern",
                "specify pattern, if the option isn't used the pattern must precede the files, \
                 the option allows the pattern to be put after the files for more convenient \
                 editing",
                "PATTERN");
    opts.optopt("r",
                "replace",
                "replace matches, may include named groups",
                "REPLACEMENT");
    // opts.optopt("n", "number", "match/replace N occurrences", "N");
    // opts.optopt("k",
    // "skip",
    // "skip N occurrences before matching/replacing",
    // "N");
    // opts.optflag("b",
    // "backwards",
    // "-n --number and -k --skip options count backwards");
    opts.optflag("i", "ignore-case", "ignore case");
    opts.optflag("s",
                 "single",
                 ". matches newlines, ^ and $ match beginning and end of each file");
    opts.optflag("m",
                 "multiline",
                 "multiline, ^ and $ match beginning and end of each line");
    opts.optflag("l", "line-oriented", "operate on lines");
    opts.optflag("x", "extended", "ignore whitespace and # comments");
    opts.optflag("o", "only-matches", "show only matches");
    opts.optopt("g",
                "group",
                "show the match group, specified by number or name",
                "GROUP");
    opts.optflag("v", "no-match", "show only non-matching");
    opts.optflag("R", "recursive", "recurse");
    opts.optflag("f", "follow", "follow symlinks");
    opts.optmulti("", "include", "match only files that match GLOB", "GLOB");
    opts.optmulti("", "exclude", "skip files matching GLOB", "GLOB");
    opts.optmulti("", "exclude-dir", "skip directories matching GLOB", "GLOB");
    opts.optflag("c", "colors", "show matches in color");
    opts.optflag("", "stdout", "output to stdout");
    opts.optflag("q", "quiet", "suppress all normal output");
    opts.optflag("a", "all", "do not ignore entries starting with .");
    opts.optflag("V", "version", "output version information and exit");
    opts.optflag("h", "help", "print this help menu and exit");
    opts
}

fn add_re_options_to_pattern(matches: &Matches, pattern: &str) -> String {
    let mut options: String = "".to_string();
    for option in vec!["i", "s", "m", "x"] {
        if matches.opt_present(&option) {
            options.push_str(&option);
        }
    }
    if options != "" {
        format!("(?{}){}", &options, &pattern)
    } else {
        pattern.to_string()
    }
}

fn get_files<T, F>(matches: &Matches,
                   file_names: &Vec<&String>,
                   make_file: F)
                   -> Result<Vec<T>, String>
    where T: Clone,
          F: Fn(&Path) -> T
{
    let stdin = file_names.len() == 0;

    let mut files = Vec::<T>::new();
    if !stdin {
        let follow = matches.opt_present("follow");
        let recursive = matches.opt_present("recursive");
        let all = matches.opt_present("all");
        let mut includes = Vec::<Pattern>::new();
        for include in matches.opt_strs("include") {
            let pattern = try!(Pattern::new(&include).map_err(|e| e.to_string()));
            includes.push(pattern);
        }
        let mut excludes = Vec::<Pattern>::new();
        for exclude in matches.opt_strs("exclude") {
            let pattern = try!(Pattern::new(&exclude).map_err(|e| e.to_string()));
            excludes.push(pattern);
        }
        let mut exclude_dir = Vec::<Pattern>::new();
        for exclude in matches.opt_strs("exclude-dir") {
            let pattern = try!(Pattern::new(&exclude).map_err(|e| e.to_string()));
            exclude_dir.push(pattern);
        }
        for file_name in file_names {
            let mut walkdir = WalkDir::new(file_name).follow_links(follow);
            if !recursive {
                walkdir = walkdir.max_depth(1);
            }
            // Filter on excludes.
            for entry in walkdir.into_iter().filter_entry(|entry| {
                let path = entry.path();
                if let Some(file_name) = path.file_name() {
                    return if let Some(file_name) = file_name.to_str() {
                        !entry.file_type().is_dir() ||
                        !(exclude_dir.iter().any(|pattern| pattern.matches(file_name)) ||
                          !all && file_name.starts_with("."))
                    } else {
                        false
                    };
                }
                true
            }) {
                let entry = try!(entry.map_err(|e| e.to_string()));
                let path = entry.path();
                if let Some(file_name) = path.file_name() {
                    if let Some(file_name) = file_name.to_str() {
                        if entry.file_type().is_file() &&
                           (includes.len() == 0 ||
                            includes.iter().any(|pattern| pattern.matches(file_name))) &&
                           !(excludes.iter().any(|pattern| pattern.matches(file_name)) ||
                             !all && file_name.starts_with(".")) {
                            println!("{:?}", path);
                            files.push(make_file(&path).clone());
                        }
                    }
                }
            }
        }
    }
    Ok(files)
}

fn process_files(matches: &Matches,
                 re: Regex,
                 files: &mut Vec<Source>,
                 mut output: &mut Write)
                 -> Result<i32, String> {
    let mut exit_code = 0;
    for mut file in files {
        exit_code = try!(process_file(&matches, &re, &mut file, &mut output));
    }
    try!(output.flush().map_err(|e| e.to_string()));
    Ok(exit_code)
}

fn process_file(matches: &Matches,
                re: &Regex,
                file: &mut Source,
                mut output: &mut Write)
                -> Result<i32, String> {

    let mut exit_code = 0;
    let color = Red.bold();

    let mut content;
    {
        let read: &mut Read = match file {
            &mut Source::Stdin(ref mut read) => read,
            &mut Source::File(ref mut file) => file,
            #[cfg(test)]
            &mut Source::Cursor(ref mut cursor) => cursor,
        };
        let mut buffer = Vec::new();
        let _ = try!(read.read_to_end(&mut buffer).map_err(|e| e.to_string()));
        content = try!(String::from_utf8(buffer).map_err(|e| e.to_string()));
    }

    let group = matches.opt_str("group");
    let line_oriented = matches.opt_present("line-oriented");
    let no_match = matches.opt_present("no-match");
    let only_matches = matches.opt_present("only-matches");
    let quiet = matches.opt_present("quiet");
    let replace = matches.opt_str("replace");
    let stdout = matches.opt_present("stdout");
    let colors = matches.opt_present("colors") && (stdout || replace.is_none());

    if let Some(mut replace) = replace {
        if colors {
            replace = color.paint(replace.as_str()).to_string();
        }
        content = re.replace_all(&content, replace.as_str());
        if stdout {
            if !quiet {
                try!(output.write(&content.into_bytes()).map_err(|e| e.to_string()));
            }
        } else {
            match file {
                &mut Source::File(ref mut file) => {
                    try!(file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string()));
                    try!(file.write(&content.into_bytes()).map_err(|e| e.to_string()));
                }
                #[cfg(test)]
                &mut Source::Cursor(ref mut file) => {
                    try!(file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string()));
                    try!(file.write(&content.into_bytes()).map_err(|e| e.to_string()));
                }
                _ => {}
            }
        }
        return Ok(exit_code);
    }

    if quiet {
        // Quiet match only is shortcut by the more performant is_match() .
        exit_code = if re.is_match(&content) {
            0
        } else {
            1
        };
    } else {
        let mut process_text = |pre: &str, text: &str, post: &str| -> Result<i32, String> {
            if let Some(ref group) = group {
                if let Some(captures) = re.captures(&text) {
                    try!(output.write(&pre.to_string().into_bytes())
                               .map_err(|e| e.to_string()));
                    match group.trim().parse::<usize>() {
                        Ok(index) => {
                            // if there are captures exit_code = 1
                            if let Some(matched) = captures.at(index) {
                                let mut matched = matched.to_string();
                                if colors {
                                    matched = re.replace_all(&matched,
                                                             color.paint("$0")
                                                                  .to_string()
                                                                  .as_str());
                                }
                                try!(output.write(&matched.to_string().into_bytes())
                                           .map_err(|e| e.to_string()));
                            }
                        }
                        Err(_) => {
                            if let Some(matched) = captures.name(group) {
                                let mut matched = matched.to_string();
                                if colors {
                                    matched = re.replace_all(&matched,
                                                             color.paint("$0")
                                                                  .to_string()
                                                                  .as_str());
                                }
                                try!(output.write(&matched.to_string().into_bytes())
                                           .map_err(|e| e.to_string()));
                            }
                        }
                    }
                    try!(output.write(&post.to_string().into_bytes())
                               .map_err(|e| e.to_string()));
                }
                Ok(0)
            } else if no_match {
                if !re.is_match(&text) {
                    try!(output.write(&pre.to_string().into_bytes())
                               .map_err(|e| e.to_string()));
                    try!(output.write(&text.to_string().into_bytes())
                               .map_err(|e| e.to_string()));
                    try!(output.write(&post.to_string().into_bytes())
                               .map_err(|e| e.to_string()));
                }
                Ok(0)
            } else if re.is_match(&text) {
                try!(output.write(&pre.to_string().into_bytes())
                           .map_err(|e| e.to_string()));
                if only_matches {
                    for (start, end) in re.find_iter(&text) {
                        let mut matched = text[start..end].to_string();
                        if colors {
                            matched = re.replace_all(&matched,
                                                     color.paint("$0").to_string().as_str());
                        }
                        try!(output.write(&matched.to_string().into_bytes())
                                   .map_err(|e| e.to_string()));
                    }
                } else {
                    let mut text = text.to_string();
                    if colors {
                        text = re.replace_all(&text, color.paint("$0").to_string().as_str());
                    }
                    try!(output.write(&text.to_string().into_bytes()).map_err(|e| e.to_string()));
                }
                try!(output.write(&post.to_string().into_bytes())
                           .map_err(|e| e.to_string()));
                Ok(0)
            } else {
                Ok(1)
            }
        };

        if line_oriented {
            for (line_number, line) in content.lines().enumerate() {
                let pre = if line_number == 0 && line.starts_with("\n") {
                    "\n"
                } else {
                    ""
                };
                try!(process_text(pre, &line, "\n"));
            }
        } else {
            try!(process_text("", &content, ""));
        }
    }
    Ok(exit_code)
}

extern crate getopts;
extern crate regex;

use getopts::Options;
use regex::Regex;
use std::{env, path, process};

fn main() {

    let args: Vec<String> = env::args().collect();
    let program =
        path::Path::new(&args[0])
        .file_name()
        .expect("Bug, could't get bin name.")
        .to_str()
        .expect("Bug, could't get bin name.");
    let args: Vec<&String> = args.iter().skip(1).collect();

    let mut opts = Options::new();
    opts.optflag("i", "in-place", "edit files in place");
    opts.optflag("n", "quiet", "output version information and exit");
    opts.optflag("v",  "version", "output version information and exit");
    opts.optflag("h", "help", "print this help menu and exit");

    let opts = opts;
    let parsed = opts.parse(&args);
    if let Err(err) = parsed {
        println!("ned: {}", err.to_string());
        process::exit(1);
    }

    let matches = parsed.unwrap();
    if matches.free.len() == 0 || matches.opt_present("h") {
        let brief = format!("Usage: {} [options] <pattern> [files]", program);
        print!("{}", opts.usage(&brief));
        return;
    }

    let pattern = &matches.free[0];
    let files: Vec<&String> = matches.free.iter().skip(1).collect();

    let re = Regex::new(&pattern);
    if let Err(err) = re {
        println!("ned: {}", err.to_string());
        process::exit(1);
    }

    // Turn files into a collection of file handles.
    // If there are no files the collection will contain just stdin, stdout.

    let re = re.unwrap();
    println!("p: {}", pattern);
    for file in files {
        // Read each file, apply the pattern, write the file.
        println!("f: {}", file);
    }
}
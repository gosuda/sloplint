//! Python import extraction: scope, test marking, reexports, external roots.

use std::borrow::Cow;

use sloplint_core::facts::{Import, ImportKind, ImportScope};
use tree_sitter::Node;

use crate::hooks::{Cx, child_by_field, last_segment, named_children, pos_of, text};

use super::{inside_function, is_test_path};

pub(crate) fn external_root_of(module: &str) -> Option<&str> {
    if module.starts_with('.') {
        return None;
    }
    let top = module.split('.').next().unwrap_or("");
    if top.is_empty() || is_stdlib(top) {
        return None;
    }
    Some(top)
}

fn scope_of(cx: &Cx<'_, '_>) -> ImportScope {
    if inside_function(cx) {
        ImportScope::FunctionLocal
    } else {
        ImportScope::Module
    }
}

fn plain_import<'s>(
    cx: &Cx<'_, 's>,
    imp: (Node<'_>, &'s str, &'s str, Option<&'s str>),
    is_reexport: bool,
    out: &mut Vec<Import<'s>>,
) {
    let (node, module, name, alias) = imp;
    out.push(Import {
        pos: pos_of(node),
        module: Cow::Borrowed(module),
        external_root: external_root_of(module),
        name,
        alias,
        kind: ImportKind::Named,
        scope: scope_of(cx),
        in_test: is_test_path(cx.path),
        is_reexport,
    });
}

fn alias_unless_same<'s>(path: &'s str, alias: &'s str) -> Option<&'s str> {
    if alias == last_segment(path, &["."]) {
        return None;
    }
    Some(alias)
}

fn reexport_here(cx: &Cx<'_, '_>, n: Node<'_>) -> bool {
    cx.path.ends_with("__init__.py") && n.kind() == "import_from_statement"
}

pub(crate) fn push_imports<'s>(n: Node<'_>, cx: &Cx<'_, 's>, out: &mut Vec<Import<'s>>) {
    if n.kind() == "import_statement" {
        for c in named_children(n) {
            if c.kind() == "dotted_name" {
                let full = text(cx, c);
                plain_import(cx, (c, full, last_segment(full, &["."]), None), false, out);
            } else if c.kind() == "aliased_import" {
                let path = child_by_field(c, "name").map_or("", |v| text(cx, v));
                let alias = child_by_field(c, "alias").map_or("", |v| text(cx, v));
                plain_import(
                    cx,
                    (
                        c,
                        path,
                        last_segment(path, &["."]),
                        alias_unless_same(path, alias),
                    ),
                    false,
                    out,
                );
            }
        }
        return;
    }
    if n.kind() != "import_from_statement" {
        return;
    }
    let module = child_by_field(n, "module_name").map_or("", |m| text(cx, m));
    let mod_id = child_by_field(n, "module_name").map(|m| m.id());
    let reexport = reexport_here(cx, n);
    for c in named_children(n) {
        if Some(c.id()) == mod_id {
            continue;
        }
        if c.kind() == "wildcard_import" {
            out.push(Import {
                pos: pos_of(c),
                module: Cow::Borrowed(module),
                external_root: external_root_of(module),
                name: "*",
                alias: None,
                kind: ImportKind::Wildcard,
                scope: scope_of(cx),
                in_test: is_test_path(cx.path),
                is_reexport: reexport,
            });
        } else if c.kind() == "aliased_import" {
            let path = child_by_field(c, "name").map_or("", |v| text(cx, v));
            let alias = child_by_field(c, "alias").map_or("", |v| text(cx, v));
            plain_import(
                cx,
                (
                    c,
                    module,
                    last_segment(path, &["."]),
                    alias_unless_same(path, alias),
                ),
                reexport,
                out,
            );
        } else if c.kind() == "dotted_name" {
            let nm = text(cx, c);
            plain_import(
                cx,
                (c, module, last_segment(nm, &["."]), None),
                reexport,
                out,
            );
        }
    }
}

pub(crate) fn is_stdlib(top: &str) -> bool {
    STDLIB.contains(&top)
}

const STDLIB: &[&str] = &[
    "__future__",
    "abc",
    "aifc",
    "argparse",
    "array",
    "ast",
    "asynchat",
    "asyncio",
    "asyncore",
    "atexit",
    "audioop",
    "base64",
    "bdb",
    "binascii",
    "bisect",
    "builtins",
    "bz2",
    "calendar",
    "cgi",
    "cgitb",
    "chunk",
    "cmath",
    "cmd",
    "code",
    "codecs",
    "codeop",
    "collections",
    "colorsys",
    "compileall",
    "concurrent",
    "configparser",
    "contextlib",
    "contextvars",
    "copy",
    "copyreg",
    "crypt",
    "csv",
    "ctypes",
    "curses",
    "dataclasses",
    "datetime",
    "dbm",
    "decimal",
    "difflib",
    "dis",
    "doctest",
    "email",
    "encodings",
    "enum",
    "errno",
    "faulthandler",
    "fcntl",
    "filecmp",
    "fileinput",
    "fnmatch",
    "fractions",
    "ftplib",
    "functools",
    "gc",
    "getopt",
    "getpass",
    "gettext",
    "glob",
    "graphlib",
    "grp",
    "gzip",
    "hashlib",
    "heapq",
    "hmac",
    "html",
    "http",
    "idlelib",
    "imaplib",
    "imghdr",
    "imp",
    "importlib",
    "inspect",
    "io",
    "ipaddress",
    "itertools",
    "json",
    "keyword",
    "lib2to3",
    "linecache",
    "locale",
    "logging",
    "lzma",
    "mailbox",
    "marshal",
    "math",
    "mimetypes",
    "mmap",
    "modulefinder",
    "multiprocessing",
    "netrc",
    "nis",
    "nntplib",
    "numbers",
    "operator",
    "optparse",
    "os",
    "pathlib",
    "pdb",
    "pickle",
    "pickletools",
    "pipes",
    "pkgutil",
    "platform",
    "plistlib",
    "poplib",
    "posix",
    "pprint",
    "profile",
    "pstats",
    "pty",
    "pwd",
    "py_compile",
    "pyclbr",
    "pydoc",
    "queue",
    "quopri",
    "random",
    "re",
    "readline",
    "reprlib",
    "resource",
    "rlcompleter",
    "runpy",
    "sched",
    "secrets",
    "select",
    "selectors",
    "shelve",
    "shlex",
    "shutil",
    "signal",
    "site",
    "smtpd",
    "smtplib",
    "sndhdr",
    "socket",
    "socketserver",
    "sqlite3",
    "ssl",
    "stat",
    "statistics",
    "string",
    "stringprep",
    "struct",
    "subprocess",
    "symtable",
    "sys",
    "sysconfig",
    "syslog",
    "tabnanny",
    "tarfile",
    "telnetlib",
    "tempfile",
    "termios",
    "textwrap",
    "threading",
    "time",
    "timeit",
    "tkinter",
    "token",
    "tokenize",
    "tomllib",
    "trace",
    "traceback",
    "tracemalloc",
    "tty",
    "turtle",
    "types",
    "typing",
    "unicodedata",
    "unittest",
    "urllib",
    "uuid",
    "venv",
    "warnings",
    "wave",
    "weakref",
    "webbrowser",
    "wsgiref",
    "xml",
    "xmlrpc",
    "zipapp",
    "zipfile",
    "zipimport",
    "zlib",
    "zoneinfo",
];

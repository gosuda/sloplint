open Result.Syntax

type dialect = Posix | Cmd
type verdict = Clear | Blocked of string | Review of string

let platform = if Sys.win32 then Cmd else Posix
let max_depth = 8

let refused_programs =
  [
    "alias";
    "aria2c";
    "axel";
    "chrome";
    "curl";
    "curlie";
    "firefox";
    "httpie";
    "http-prompt";
    "links";
    "lynx";
    "nc";
    "safari";
    "scp";
    "ssh";
    "telnet";
    "w3m";
    "wget";
    "xh";
    "doas";
    "su";
    "sudo";
    "apk";
    "apt";
    "apt-cache";
    "apt-get";
    "dnf";
    "dpkg";
    "emerge";
    "home-manager";
    "makepkg";
    "opkg";
    "pacman";
    "paru";
    "pkg";
    "pkg_add";
    "pkg_delete";
    "portage";
    "rpm";
    "yay";
    "yum";
    "zypper";
    "at";
    "batch";
    "chkconfig";
    "crontab";
    "fdisk";
    "mkfs";
    "mount";
    "parted";
    "service";
    "systemctl";
    "umount";
    "firewall-cmd";
    "ifconfig";
    "ip";
    "iptables";
    "netstat";
    "pfctl";
    "route";
    "ufw";
    "kill";
    "pkill";
    "killall";
    "shutdown";
    "reboot";
    "poweroff";
    "halt";
    "eval";
    "exec";
    "source";
  ]

let refused_sequences =
  [
    [ "apk"; "add" ];
    [ "apt"; "install" ];
    [ "apt-get"; "install" ];
    [ "dnf"; "install" ];
    [ "pacman"; "-s" ];
    [ "pkg"; "install" ];
    [ "yum"; "install" ];
    [ "zypper"; "install" ];
    [ "brew"; "install" ];
    [ "cargo"; "install" ];
    [ "gem"; "install" ];
    [ "go"; "install" ];
    [ "npm"; "install"; "--global" ];
    [ "npm"; "install"; "-g" ];
    [ "pip"; "install"; "--user" ];
    [ "pip3"; "install"; "--user" ];
    [ "pnpm"; "add"; "--global" ];
    [ "pnpm"; "add"; "-g" ];
    [ "yarn"; "global"; "add" ];
    [ "go"; "test"; "-exec" ];
  ]

let posix_shells =
  [
    "sh";
    "bash";
    "rbash";
    "dash";
    "zsh";
    "ksh";
    "ksh93";
    "mksh";
    "pdksh";
    "ash";
    "yash";
    "posh";
  ]

let foreign_shells = [ "fish"; "csh"; "tcsh" ]
let code_runners = [ "."; "eval"; "source" ]

let wrappers =
  [
    "command";
    "builtin";
    "exec";
    "time";
    "coproc";
    "hash";
    "enable";
    "env";
    "nohup";
    "nice";
    "timeout";
    "stdbuf";
    "setsid";
    "busybox";
    "ionice";
    "chrt";
    "taskset";
    "unbuffer";
    "chroot";
    "flock";
    "runuser";
    "script";
    "doas";
    "sudo";
    "su";
  ]

let fd_exec_flags = [ "-x"; "-X"; "--exec"; "--exec-batch" ]

let fd_data_flags =
  [ "-e"; "--extension"; "-E"; "--exclude"; "-t"; "--type"; "-d"; "--max-depth" ]

let fd_attached_data value =
  List.exists
    (fun flag ->
      let prefix = if String.starts_with ~prefix:"--" flag then flag ^ "=" else flag in
      String.length value > String.length prefix && String.starts_with ~prefix value)
    fd_data_flags

let rec fd_executes = function
  | flag :: _operand :: rest when List.mem flag fd_data_flags -> fd_executes rest
  | "--" :: _ | [] -> false
  | flag :: rest when fd_attached_data flag -> fd_executes rest
  | flag :: rest -> List.mem flag fd_exec_flags || fd_executes rest

let exec_flags =
  [ ("find", [ "-exec"; "-execdir"; "-ok"; "-okdir" ]); ("fd", fd_exec_flags) ]

let code_flags =
  [
    ("env", [ "-S"; "--split-string" ]);
    ("flock", [ "-c"; "--command" ]);
    ("runuser", [ "-c"; "--command" ]);
    ("script", [ "-c"; "--command" ]);
  ]

let cmd_runners = [ "if"; "for"; "do"; "else"; "call"; "start"; "cmd" ]
let executable_suffixes = [ ".exe"; ".com"; ".bat"; ".cmd" ]

let reserved_starters =
  [ "!"; "{"; "}"; "if"; "then"; "else"; "elif"; "fi"; "do"; "done"; "while"; "until" ]

let computed_program = "the shell computes the name of the program it runs"
let for_without_do = "a for loop without do"
let for_command_source = "a for /f loop runs a command the screen does not parse"
let sourced_code = "the command runs shell code the screen cannot read"
let stdin_code = "a shell reads its code from standard input"
let computed_code = "a shell runs a computed command string"
let foreign_code = "the screen cannot read code written for this shell"
let too_deep = "commands nest too deeply to screen"
let unreadable message = "the command cannot be parsed: " ^ message
let is_letter c = (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')
let is_digit c = c >= '0' && c <= '9'
let is_name_start c = is_letter c || c = '_'
let is_name_char c = is_name_start c || is_digit c
let is_all_digits text = text <> "" && String.for_all is_digit text
let suffix_from text start = String.sub text start (String.length text - start)

let contains_substring ~needle haystack =
  let width = String.length needle in
  let limit = String.length haystack - width in
  let rec at index =
    index <= limit
    && (String.equal (String.sub haystack index width) needle || at (index + 1))
  in
  width > 0 && at 0

let is_fragment_char dialect c =
  is_letter c || is_digit c
  ||
  match c with
  | '_' | '-' | '.' | '/' | ':' -> true
  | '=' -> ( match dialect with Posix -> true | Cmd -> false)
  | _ -> false

let fragments dialect text =
  let words = ref [] in
  let current = Buffer.create 16 in
  let flush () =
    if Buffer.length current > 0 then (
      words := Buffer.contents current :: !words;
      Buffer.clear current)
  in
  String.iter
    (fun c ->
      if is_fragment_char dialect c then Buffer.add_char current (Char.lowercase_ascii c)
      else flush ())
    text;
  flush ();
  List.rev !words

let drop_leading c text =
  let length = String.length text in
  let rec first index =
    if index < length && text.[index] = c then first (index + 1) else index
  in
  suffix_from text (first 0)

let basename text =
  let name = String.lowercase_ascii text in
  let start =
    match (String.rindex_opt name '/', String.rindex_opt name '\\') with
    | Some slash, Some backslash -> max slash backslash + 1
    | Some index, None | None, Some index -> index + 1
    | None, None -> 0
  in
  drop_leading '=' (suffix_from name start)

let strip_executable_suffix base =
  match
    List.find_opt
      (fun suffix ->
        String.length base > String.length suffix && String.ends_with ~suffix base)
      executable_suffixes
  with
  | Some suffix -> String.sub base 0 (String.length base - String.length suffix)
  | None -> base

let rec has_sequence name_of wanted names =
  match (wanted, names) with
  | [], _ -> true
  | _ :: _, [] -> false
  | first :: rest, name :: tail ->
      if String.equal (name_of name) first then has_sequence name_of rest tail
      else has_sequence name_of wanted tail

let refused names =
  match List.find_opt (fun name -> List.mem name refused_programs) names with
  | Some name -> Some name
  | None ->
      List.find_opt (fun sequence -> has_sequence Fun.id sequence names) refused_sequences
      |> Option.map (String.concat " ")

type word = {
  value : string;
  fixed : bool;
  unquoted : bool;
  assignment : bool;
  single : bool;
}
(* [single] holds when the word provably expands to exactly one argument: it has
   no unquoted expansion, glob, or brace expansion that could split it into
   several fields. A quoted expansion such as ["$pattern"] is single, while an
   unquoted [$pattern] is not. Data positions for fixed-arity predicates (such
   as [find -name]) accept single words; anything else still requires review. *)

type builder = {
  buf : Buffer.t;
  mutable is_fixed : bool;
  mutable is_unquoted : bool;
  mutable is_assignment : bool;
  mutable is_single : bool;
  mutable seen_equals : bool;
  mutable braces : int;
  mutable brace_list : bool;
  mutable bracket : bool;
}

let builder () =
  {
    buf = Buffer.create 16;
    is_fixed = true;
    is_unquoted = true;
    is_assignment = false;
    is_single = true;
    seen_equals = false;
    braces = 0;
    brace_list = false;
    bracket = false;
  }

let word_of b =
  {
    value = Buffer.contents b.buf;
    fixed = b.is_fixed;
    unquoted = b.is_unquoted;
    assignment = b.is_assignment;
    single = b.is_single;
  }

let is_name text =
  text <> "" && is_name_start text.[0] && String.for_all is_name_char text

let is_assignment_name name =
  if String.ends_with ~suffix:"+" name then
    is_name (String.sub name 0 (String.length name - 1))
  else is_name name

let is_brace_name text =
  let length = String.length text in
  length > 2
  && text.[0] = '{'
  && text.[length - 1] = '}'
  && is_name (String.sub text 1 (length - 2))

let follows_dot b =
  let length = Buffer.length b.buf in
  length > 0 && Buffer.nth b.buf (length - 1) = '.'

let add_unquoted b c =
  (match c with
  | '*' | '?' ->
      b.is_fixed <- false;
      b.is_single <- false
  | '[' -> b.bracket <- true
  | ']' ->
      if b.bracket then (
        b.is_fixed <- false;
        b.is_single <- false)
  | '{' -> b.braces <- b.braces + 1
  | ',' -> if b.braces > 0 then b.brace_list <- true
  | '.' -> if b.braces > 0 && follows_dot b then b.brace_list <- true
  | '}' ->
      if b.braces > 0 then (
        b.braces <- b.braces - 1;
        if b.brace_list then (
          b.is_fixed <- false;
          b.is_single <- false))
  | '=' ->
      if not b.seen_equals then (
        b.seen_equals <- true;
        b.is_assignment <- b.is_unquoted && is_assignment_name (Buffer.contents b.buf))
  | _ -> ());
  Buffer.add_char b.buf c

let add_quoted b c =
  Buffer.add_char b.buf c;
  b.is_unquoted <- false

let add_quoted_string b text =
  Buffer.add_string b.buf text;
  b.is_unquoted <- false

let mark_expansion b ~single =
  Buffer.add_char b.buf '\000';
  b.is_fixed <- false;
  b.is_unquoted <- false;
  b.is_single <- b.is_single && single

type sink = {
  mutable words : string list;
  mutable commands : (int * word list) list;
  mutable reviews : string list;
  mutable refused_executable : string option;
}

let review sink reason = sink.reviews <- reason :: sink.reviews

type heredoc = { delimiter : string; expands : bool; strip_tabs : bool }
type token = Word of word | Op of string | Redirect | End

type lexer = {
  source : string;
  mutable pos : int;
  depth : int;
  ansi_quotes : bool;
  sink : sink;
  mutable delimiter_next : bool option;
  mutable heredocs : heredoc list;
  mutable nesting : int;
}

let lexer sink ~depth ~ansi_quotes source =
  {
    source;
    pos = 0;
    depth;
    ansi_quotes;
    sink;
    delimiter_next = None;
    heredocs = [];
    nesting = 0;
  }

let max_nesting = 64

let nest lx expand =
  if lx.nesting >= max_nesting then Error "expansions nest too deeply"
  else (
    lx.nesting <- lx.nesting + 1;
    let result = expand () in
    lx.nesting <- lx.nesting - 1;
    result)

let peek lx offset =
  let index = lx.pos + offset in
  if index < String.length lx.source then Some lx.source.[index] else None

let advance lx count = lx.pos <- min (String.length lx.source) (lx.pos + count)

let is_metachar = function
  | ' ' | '\t' | '\n' | ';' | '&' | '|' | '(' | ')' | '<' | '>' -> true
  | _ -> false

let rec skip_blanks lx =
  match (peek lx 0, peek lx 1) with
  | Some (' ' | '\t'), _ ->
      advance lx 1;
      skip_blanks lx
  | Some '\\', Some '\n' ->
      advance lx 2;
      skip_blanks lx
  | _ -> ()

let rec skip_comment lx =
  match peek lx 0 with
  | None | Some '\n' -> ()
  | Some _ ->
      advance lx 1;
      skip_comment lx

let rec skip_name lx =
  match peek lx 0 with
  | Some c when is_name_char c ->
      advance lx 1;
      skip_name lx
  | Some _ | None -> ()

let operators =
  [
    "&&";
    "&";
    "||";
    "|";
    ";;&";
    ";;";
    ";&";
    ";";
    "(";
    ")";
    "<<<";
    "<<-";
    "<<";
    "<&";
    "<>";
    "<";
    ">>";
    ">&";
    ">|";
    ">";
  ]

let operator lx =
  let starts_here op =
    let length = String.length op in
    lx.pos + length <= String.length lx.source
    && String.equal (String.sub lx.source lx.pos length) op
  in
  let op =
    match List.find_opt starts_here operators with
    | Some op -> op
    | None -> String.sub lx.source lx.pos 1
  in
  advance lx (String.length op);
  match op with
  | "<<" | "<<-" ->
      lx.delimiter_next <- Some (String.equal op "<<-");
      Redirect
  | _ when op.[0] = '<' || op.[0] = '>' -> Redirect
  | _ -> Op op

let record_word lx w =
  lx.sink.words <- w.value :: lx.sink.words;
  match lx.delimiter_next with
  | None -> ()
  | Some strip_tabs ->
      lx.heredocs <-
        { delimiter = w.value; expands = w.unquoted; strip_tabs } :: lx.heredocs;
      lx.delimiter_next <- None

let trailing_backslashes line =
  let rec loop index count =
    if index < 0 then count
    else if line.[index] = '\\' then loop (index - 1) (count + 1)
    else count
  in
  loop (String.length line - 1) 0

let read_body lx doc =
  let length = String.length lx.source in
  let physical () =
    if lx.pos >= length then None
    else
      let stop =
        Option.value (String.index_from_opt lx.source lx.pos '\n') ~default:length
      in
      let line = String.sub lx.source lx.pos (stop - lx.pos) in
      lx.pos <- min length (stop + 1);
      Some line
  in
  (* An expanding here-document joins a line ending in an unescaped backslash
     with the next line before comparing against the delimiter, so the screen
     assembles the same logical lines the shell does. Only an odd trailing
     count continues the line; an escaped pair is a literal backslash. *)
  let rec lines acc pending =
    match physical () with
    | None ->
        review lx.sink (unreadable "unterminated here-document");
        List.rev acc
    | Some line when doc.expands && trailing_backslashes line mod 2 = 1 ->
        review lx.sink (unreadable "a here-document contains a line continuation");
        lines acc (pending ^ String.sub line 0 (String.length line - 1))
    | Some line ->
        let line = pending ^ line in
        let line = if doc.strip_tabs then drop_leading '\t' line else line in
        if String.equal line doc.delimiter then List.rev acc else lines (line :: acc) ""
  in
  String.concat "\n" (lines [] "")

let digit_value ~base c =
  let value =
    match c with
    | '0' .. '9' -> Char.code c - Char.code '0'
    | 'a' .. 'f' -> Char.code c - Char.code 'a' + 10
    | 'A' .. 'F' -> Char.code c - Char.code 'A' + 10
    | _ -> base
  in
  if value < base then Some value else None

let read_number lx ~base ~limit =
  let rec loop value count =
    if count = limit then (value, count)
    else
      match Option.bind (peek lx 0) (digit_value ~base) with
      | Some digit ->
          advance lx 1;
          loop ((value * base) + digit) (count + 1)
      | None -> (value, count)
  in
  loop 0 0

let decode_escape lx b escape =
  match escape with
  | 'a' -> Buffer.add_char b.buf '\007'
  | 'b' -> Buffer.add_char b.buf '\b'
  | 'e' | 'E' -> Buffer.add_char b.buf '\027'
  | 'f' -> Buffer.add_char b.buf '\012'
  | 'n' -> Buffer.add_char b.buf '\n'
  | 'r' -> Buffer.add_char b.buf '\r'
  | 't' -> Buffer.add_char b.buf '\t'
  | 'v' -> Buffer.add_char b.buf '\011'
  | '\\' | '\'' | '"' | '?' -> Buffer.add_char b.buf escape
  | '0' .. '7' ->
      let rest, count = read_number lx ~base:8 ~limit:2 in
      let value = ((Char.code escape - Char.code '0') lsl (3 * count)) + rest in
      Buffer.add_char b.buf (Char.chr (value land 0xff))
  | 'x' -> (
      match read_number lx ~base:16 ~limit:2 with
      | _, 0 -> Buffer.add_string b.buf "\\x"
      | value, _ -> Buffer.add_char b.buf (Char.chr value))
  | 'u' | 'U' -> (
      let limit = if escape = 'u' then 4 else 8 in
      match read_number lx ~base:16 ~limit with
      | _, 0 ->
          Buffer.add_char b.buf '\\';
          Buffer.add_char b.buf escape
      | value, _ ->
          if Uchar.is_valid value then Buffer.add_utf_8_uchar b.buf (Uchar.of_int value))
  | 'c' -> (
      match peek lx 0 with
      | Some c ->
          advance lx 1;
          Buffer.add_char b.buf (Char.chr (Char.code c land 0x1f))
      | None -> Buffer.add_string b.buf "\\c")
  | _ ->
      Buffer.add_char b.buf '\\';
      Buffer.add_char b.buf escape

let ansi_c lx b =
  b.is_unquoted <- false;
  let rec read truncated =
    match peek lx 0 with
    | None -> Error "unterminated $'...' string"
    | Some '\'' ->
        advance lx 1;
        Ok ()
    | Some '\\' -> (
        match peek lx 1 with
        | None -> Error "unterminated $'...' string"
        | Some escape ->
            let start = Buffer.length b.buf in
            advance lx 2;
            decode_escape lx b escape;
            let nul = Buffer.length b.buf > start && Buffer.nth b.buf start = '\000' in
            if nul then review lx.sink (unreadable "an ANSI-C quote contains NUL");
            if truncated || nul then Buffer.truncate b.buf start;
            read (truncated || nul))
    | Some c ->
        if not truncated then Buffer.add_char b.buf c;
        advance lx 1;
        read truncated
  in
  read false

let single_quoted lx b =
  match String.index_from_opt lx.source lx.pos '\'' with
  | None -> Error "unterminated single quote"
  | Some stop ->
      add_quoted_string b (String.sub lx.source lx.pos (stop - lx.pos));
      lx.pos <- stop + 1;
      Ok ()

type mode =
  | Command
  | Argument
  | Case_subject
  | Case_word
  | Pattern
  | For_name
  | For_words
  | Function_name

type frame = Group | Resume of word list * mode

type parser = {
  mutable mode : mode;
  mutable redirected : bool;
  mutable current : word list;
  mutable frames : frame list;
  mutable cases : int;
}

let is_reserved w name = w.unquoted && String.equal w.value name

let finish lx p =
  match p.current with
  | [] -> ()
  | words ->
      lx.sink.commands <- (lx.depth, List.rev words) :: lx.sink.commands;
      p.current <- []

let on_command_word p w =
  if w.unquoted && List.mem w.value reserved_starters then ()
  else if is_reserved w "case" then (
    p.cases <- p.cases + 1;
    p.mode <- Case_subject)
  else if is_reserved w "for" || is_reserved w "select" then p.mode <- For_name
  else if is_reserved w "function" then p.mode <- Function_name
  else if is_reserved w "esac" then p.cases <- p.cases - 1
  else if not w.assignment then (
    p.current <- [ w ];
    p.mode <- Argument)

let on_word p w =
  match p.mode with
  | Command -> on_command_word p w
  | Argument -> p.current <- w :: p.current
  | Case_subject -> p.mode <- Case_word
  | Case_word -> p.mode <- Pattern
  | Pattern ->
      if is_reserved w "esac" then (
        p.cases <- p.cases - 1;
        p.mode <- Command)
  | For_name -> p.mode <- For_words
  | For_words -> if is_reserved w "do" then p.mode <- Command
  | Function_name -> p.mode <- Command

let close_group lx p =
  finish lx p;
  match p.frames with
  | [] -> `Close
  | Group :: rest ->
      p.frames <- rest;
      p.mode <- Command;
      `Continue
  | Resume (saved, mode) :: rest ->
      p.frames <- rest;
      p.current <- saved;
      p.mode <- mode;
      `Continue

let on_op lx p op =
  match (p.mode, op) with
  | Pattern, ("(" | "|") | (Case_subject | Case_word | Pattern), "\n" -> `Continue
  | Pattern, ")" ->
      p.mode <- Command;
      `Continue
  | _, "(" ->
      finish lx p;
      p.frames <- Group :: p.frames;
      p.mode <- Command;
      `Continue
  | _, ")" -> close_group lx p
  | _, (";;" | ";&" | ";;&") ->
      finish lx p;
      p.mode <- (if p.cases > 0 then Pattern else Command);
      `Continue
  | _, _ ->
      finish lx p;
      p.mode <- Command;
      `Continue

let contains_ansi_quote source = contains_substring ~needle:"$'" source

let double_quoted_parameter_word lx =
  let rec end_of_name offset =
    match peek lx offset with
    | Some c when is_name_char c -> end_of_name (offset + 1)
    | _ -> offset
  in
  let operator =
    match peek lx 0 with
    | Some c when is_name_char c -> end_of_name 1
    | Some ('@' | '*' | '#' | '?' | '-' | '$' | '!') -> 1
    | _ -> 0
  in
  let operator = if peek lx operator = Some ':' then operator + 1 else operator in
  operator > 0
  && match peek lx operator with Some ('-' | '+' | '=' | '?') -> true | _ -> false

let parameter_expands_fields lx =
  let rec indirect offset =
    match peek lx offset with
    | Some '@' -> true
    | Some c when is_name_char c -> indirect (offset + 1)
    | _ -> false
  in
  peek lx 0 = Some '@' || (peek lx 0 = Some '!' && indirect 1)

let rec lex_word lx b =
  match peek lx 0 with
  | None -> Ok ()
  | Some c when is_metachar c -> Ok ()
  | Some '\\' -> (
      match peek lx 1 with
      | Some '\n' ->
          advance lx 2;
          lex_word lx b
      | Some c ->
          add_quoted b c;
          advance lx 2;
          lex_word lx b
      | None ->
          add_quoted b '\\';
          advance lx 1;
          Ok ())
  | Some '\'' ->
      advance lx 1;
      let* () = single_quoted lx b in
      lex_word lx b
  | Some '"' ->
      advance lx 1;
      let* () = double_quoted lx b ~closing:true in
      lex_word lx b
  | Some '$' when Option.is_some lx.delimiter_next -> (
      match peek lx 1 with
      | Some ('\'' | '"' | '{' | '(') -> Error "unsupported here-document delimiter"
      | _ ->
          Buffer.add_char b.buf '$';
          advance lx 1;
          lex_word lx b)
  | Some '`' when Option.is_some lx.delimiter_next ->
      Error "unsupported here-document delimiter"
  | Some '$' ->
      let* () = dollar lx b ~in_double:false in
      lex_word lx b
  | Some '`' ->
      advance lx 1;
      let* () = backquoted lx b ~in_double:false in
      lex_word lx b
  | Some c ->
      add_unquoted b c;
      advance lx 1;
      lex_word lx b

and double_quoted lx b ~closing =
  b.is_unquoted <- false;
  match peek lx 0 with
  | None -> if closing then Error "unterminated double quote" else Ok ()
  | Some '"' when closing ->
      advance lx 1;
      Ok ()
  | Some '\\' ->
      (match peek lx 1 with
      | Some '\n' -> advance lx 2
      | Some (('$' | '`' | '\\') as c) ->
          add_quoted b c;
          advance lx 2
      | Some '"' when closing ->
          add_quoted b '"';
          advance lx 2
      | Some _ | None ->
          add_quoted b '\\';
          advance lx 1);
      double_quoted lx b ~closing
  | Some '$' when Option.is_none lx.delimiter_next ->
      let* () = dollar lx b ~in_double:true in
      double_quoted lx b ~closing
  | Some '`' when Option.is_none lx.delimiter_next ->
      advance lx 1;
      let* () = backquoted lx b ~in_double:closing in
      double_quoted lx b ~closing
  | Some c ->
      add_quoted b c;
      advance lx 1;
      double_quoted lx b ~closing

and dollar lx b ~in_double =
  match peek lx 1 with
  | Some '\'' when lx.ansi_quotes && not in_double ->
      advance lx 2;
      ansi_c lx b
  | Some '"' when not in_double ->
      advance lx 2;
      double_quoted lx b ~closing:true
  | Some '(' ->
      let* () =
        nest lx (fun () ->
            match peek lx 2 with
            | Some '(' ->
                advance lx 3;
                arithmetic lx (builder ()) 0
            | Some _ | None ->
                advance lx 2;
                parse_list lx ~nested:true)
      in
      Ok (mark_expansion b ~single:in_double)
  | Some '{' ->
      advance lx 2;
      let literal_apostrophe = in_double && double_quoted_parameter_word lx in
      let scratch = builder () in
      scratch.is_single <- not (parameter_expands_fields lx);
      let* () = nest lx (fun () -> parameter lx scratch ~literal_apostrophe) in
      Ok (mark_expansion b ~single:(in_double && scratch.is_single))
  | Some c when is_name_start c ->
      advance lx 2;
      skip_name lx;
      Ok (mark_expansion b ~single:in_double)
  | Some c when is_digit c || String.contains "@*#?-$!" c ->
      (* A quoted ["$@"] still expands to one word per positional parameter, and
         ["${words[@]}"] does the same for arrays; every other special
         parameter holds a single value. *)
      advance lx 2;
      Ok (mark_expansion b ~single:(in_double && c <> '@'))
  | Some _ | None ->
      Buffer.add_char b.buf '$';
      advance lx 1;
      Ok ()

and arithmetic lx scratch depth =
  match peek lx 0 with
  | None -> Error "unterminated arithmetic expansion"
  | Some '(' ->
      advance lx 1;
      arithmetic lx scratch (depth + 1)
  | Some ')' when depth > 0 ->
      advance lx 1;
      arithmetic lx scratch (depth - 1)
  | Some ')' -> (
      match peek lx 1 with
      | Some ')' ->
          advance lx 2;
          Ok ()
      | Some _ | None -> Error "unbalanced arithmetic expansion")
  | Some '$' ->
      let* () = dollar lx scratch ~in_double:true in
      arithmetic lx scratch depth
  | Some '`' ->
      advance lx 1;
      let* () = backquoted lx scratch ~in_double:false in
      arithmetic lx scratch depth
  | Some '\\' ->
      advance lx 2;
      arithmetic lx scratch depth
  | Some _ ->
      advance lx 1;
      arithmetic lx scratch depth

and parameter lx scratch ~literal_apostrophe =
  match peek lx 0 with
  | None -> Error "unterminated parameter expansion"
  | Some '}' ->
      advance lx 1;
      Ok ()
  | Some '\\' ->
      advance lx 2;
      parameter lx scratch ~literal_apostrophe
  | Some '\'' when not literal_apostrophe ->
      advance lx 1;
      let* () = single_quoted lx scratch in
      parameter lx scratch ~literal_apostrophe
  | Some '"' ->
      advance lx 1;
      let* () = double_quoted lx scratch ~closing:true in
      parameter lx scratch ~literal_apostrophe
  | Some '$' ->
      let* () = dollar lx scratch ~in_double:literal_apostrophe in
      parameter lx scratch ~literal_apostrophe
  | Some '`' ->
      advance lx 1;
      let* () = backquoted lx scratch ~in_double:literal_apostrophe in
      parameter lx scratch ~literal_apostrophe
  | Some c ->
      (* [${words[@]}] expands to one word per element even when quoted, so a
         word holding it is never a proven single argument. *)
      if c = '@' && lx.pos > 0 && lx.source.[lx.pos - 1] = '[' then
        scratch.is_single <- false;
      advance lx 1;
      parameter lx scratch ~literal_apostrophe

and backquoted lx b ~in_double =
  let body = Buffer.create 32 in
  let rec collect () =
    match peek lx 0 with
    | None -> Error "unterminated backquote"
    | Some '`' ->
        advance lx 1;
        Ok ()
    | Some '\\' ->
        (match peek lx 1 with
        | Some (('$' | '`' | '\\') as c) ->
            Buffer.add_char body c;
            advance lx 2
        | Some '"' when in_double ->
            Buffer.add_char body '"';
            advance lx 2
        | Some _ | None ->
            Buffer.add_char body '\\';
            advance lx 1);
        collect ()
    | Some c ->
        Buffer.add_char body c;
        advance lx 1;
        collect ()
  in
  let* () = collect () in
  parse_source lx.sink ~depth:(lx.depth + 1) (Buffer.contents body);
  Ok (mark_expansion b ~single:in_double)

and next_token lx =
  skip_blanks lx;
  match peek lx 0 with
  | (None | Some ('\n' | '&' | '|' | ';' | '(' | ')' | '<' | '>'))
    when Option.is_some lx.delimiter_next ->
      Error "missing here-document delimiter"
  | None when lx.heredocs <> [] -> Error "unterminated here-document"
  | None -> Ok End
  | Some '#' ->
      skip_comment lx;
      next_token lx
  | Some '\n' ->
      advance lx 1;
      read_heredocs lx;
      Ok (Op "\n")
  | Some ('&' | '|' | ';' | '(' | ')' | '<' | '>') -> Ok (operator lx)
  | Some _ ->
      let b = builder () in
      let* () = lex_word lx b in
      let w = word_of b in
      let redirects =
        match peek lx 0 with Some ('<' | '>') -> true | Some _ | None -> false
      in
      if
        Option.is_none lx.delimiter_next
        && redirects && w.unquoted
        && (is_all_digits w.value || is_brace_name w.value)
      then Ok (operator lx)
      else (
        record_word lx w;
        Ok (Word w))

and read_heredocs lx =
  let pending = List.rev lx.heredocs in
  lx.heredocs <- [];
  List.iter
    (fun doc ->
      let body = read_body lx doc in
      if doc.expands then
        expand_body lx.sink ~depth:(lx.depth + 1) ~ansi_quotes:lx.ansi_quotes body)
    pending

and expand_body sink ~depth ~ansi_quotes body =
  if depth > max_depth then review sink too_deep
  else
    let b = builder () in
    match double_quoted (lexer sink ~depth ~ansi_quotes body) b ~closing:false with
    | Ok () -> sink.words <- Buffer.contents b.buf :: sink.words
    | Error message -> review sink (unreadable message)

and parse_list lx ~nested =
  let p = { mode = Command; redirected = false; current = []; frames = []; cases = 0 } in
  let rec loop () =
    let* token = next_token lx in
    match token with
    | End -> (
        finish lx p;
        match p.frames with
        | _ when nested -> Error "unterminated command substitution"
        | [] -> Ok ()
        | _ :: _ -> Error "unbalanced parenthesis")
    | Redirect ->
        p.redirected <- true;
        loop ()
    | Word _ when p.redirected ->
        p.redirected <- false;
        loop ()
    | Word w ->
        on_word p w;
        loop ()
    | Op "(" when p.redirected ->
        p.redirected <- false;
        p.frames <- Resume (p.current, p.mode) :: p.frames;
        p.current <- [];
        p.mode <- Command;
        loop ()
    | Op op -> (
        p.redirected <- false;
        match on_op lx p op with
        | `Continue -> loop ()
        | `Close -> if nested then Ok () else Error "unbalanced parenthesis")
  in
  loop ()

and parse_source sink ~depth source =
  let parse ansi_quotes =
    match parse_list (lexer sink ~depth ~ansi_quotes source) ~nested:false with
    | Ok () -> ()
    | Error message -> review sink (unreadable message)
  in
  if depth > max_depth then review sink too_deep
  else (
    parse true;
    if contains_ansi_quote source then parse false)

let is_option w = String.length w.value > 1 && (w.value.[0] = '-' || w.value.[0] = '+')
let wrapper_syntax = "the screen cannot determine a wrapper's command"

type option_use = Flag | Operand | Unknown

let option_use ~switches ~arguments ?(optional = []) value =
  if List.mem value switches then Flag
  else if List.mem value arguments then Operand
  else if
    List.exists
      (fun option ->
        let width = String.length option in
        String.length value > width
        && String.starts_with ~prefix:option value
        && (width = 2 || value.[width] = '='))
      arguments
  then Flag
  else if
    List.exists
      (fun option ->
        String.length value > String.length option + 1
        && String.starts_with ~prefix:(option ^ "=") value)
      optional
  then Flag
  else Unknown

let rec option_prefix ~switches ~arguments = function
  | w :: _ when (not w.fixed) && not w.assignment -> Error computed_program
  | w :: rest when String.equal w.value "--" -> Ok rest
  | w :: rest when is_option w -> (
      match (option_use ~switches ~arguments w.value, rest) with
      | Flag, _ -> option_prefix ~switches ~arguments rest
      | Operand, value :: rest when value.fixed -> option_prefix ~switches ~arguments rest
      | _ -> Error wrapper_syntax)
  | words -> Ok words

let wrapper_options = function
  | "env" ->
      ( [ "-i"; "--ignore-environment"; "-v"; "--debug" ],
        [ "-u"; "--unset"; "-C"; "--chdir" ] )
  | "timeout" ->
      ( [ "--foreground"; "--preserve-status"; "-v"; "--verbose" ],
        [ "-s"; "--signal"; "-k"; "--kill-after" ] )
  | "command" -> ([ "-p" ], [])
  | "time" -> ([ "-p" ], [])
  | "nice" -> ([], [ "-n"; "--adjustment" ])
  | "stdbuf" -> ([], [ "-i"; "-o"; "-e"; "--input"; "--output"; "--error" ])
  | "setsid" -> ([ "-c"; "-f"; "-w"; "--ctty"; "--fork"; "--wait" ], [])
  | "ionice" -> ([ "-t"; "--ignore" ], [ "-c"; "--class"; "-n"; "--classdata" ])
  | "chrt" -> ([ "-b"; "-f"; "-i"; "-o"; "-r" ], [])
  | "taskset" -> ([ "-c"; "--cpu-list" ], [])
  | "flock" ->
      ( [
          "-s";
          "-x";
          "-n";
          "-u";
          "-o";
          "-F";
          "--shared";
          "--exclusive";
          "--nonblock";
          "--unlock";
          "--close";
          "--no-fork";
        ],
        [ "-w"; "--timeout"; "-E"; "--conflict-exit-code" ] )
  | "chroot" -> ([], [ "--groups"; "--userspec" ])
  | _ -> ([], [])

let rec env_command = function
  | w :: _ when (not w.fixed) && ((not w.assignment) || not w.single) ->
      Error computed_program
  | w :: rest when String.contains w.value '=' -> env_command rest
  | words -> Ok words

(* The [command] builtin has an execution mode and a lookup mode: [-v] and [-V]
   only report how the shell resolves a name without invoking it, so their
   operands are names, never commands to analyze. *)
let command_arguments args =
  let rec options lookup = function
    | w :: _ as words when not w.fixed ->
        if lookup then Ok (true, words) else Error computed_program
    | w :: rest when w.value = "--" -> Ok (lookup, rest)
    | w :: rest when String.length w.value > 1 && w.value.[0] = '-' ->
        let group = suffix_from w.value 1 in
        if String.for_all (fun c -> c = 'p' || c = 'v' || c = 'V') group then
          options (lookup || String.contains group 'v' || String.contains group 'V') rest
        else Error wrapper_syntax
    | words -> Ok (lookup, words)
  in
  options false args

let env_options args =
  let switches, arguments = wrapper_options "env" in
  let* words = option_prefix ~switches ~arguments args in
  match words with
  | word :: rest when word.fixed && word.value = "-" -> Ok rest
  | _ -> Ok words

let wrapped_command program args =
  if String.equal program "command" then
    let* lookup, words = command_arguments args in
    Ok (if lookup then [] else words)
  else
    let switches, arguments = wrapper_options program in
    let* words =
      if program = "env" then env_options args
      else option_prefix ~switches ~arguments args
    in
    match (program, words) with
    | "env", _ -> env_command words
    | "flock", _ :: mode :: _ when is_option mode -> Error wrapper_syntax
    | ("timeout" | "chroot" | "taskset" | "chrt" | "flock"), operand :: command ->
        if operand.fixed then Ok command else Error computed_program
    | ("coproc" | "script" | "runuser"), _ -> Error wrapper_syntax
    | _ -> Ok words

let xargs_switches =
  [
    "-0";
    "--null";
    "-o";
    "--open-tty";
    "-p";
    "--interactive";
    "-r";
    "--no-run-if-empty";
    "-t";
    "--verbose";
    "-x";
    "--exit";
  ]

let xargs_arguments =
  [
    "-a";
    "--arg-file";
    "-d";
    "--delimiter";
    "-E";
    "-L";
    "--max-lines";
    "-n";
    "--max-args";
    "-P";
    "--max-procs";
    "-s";
    "--max-chars";
  ]

let replacement_of flag =
  if String.equal flag "-i" || String.equal flag "--replace" then Some "{}"
  else if String.starts_with ~prefix:"--replace=" flag then Some (suffix_from flag 10)
  else if String.starts_with ~prefix:"-I" flag || String.starts_with ~prefix:"-i" flag
  then Some (suffix_from flag 2)
  else None

let xargs_grouping value =
  String.starts_with ~prefix:"-n" value
  || String.starts_with ~prefix:"-L" value
  || List.mem value [ "--max-args"; "--max-lines" ]
  || String.starts_with ~prefix:"--max-args=" value
  || String.starts_with ~prefix:"--max-lines=" value

(* Short switches that take no operand cluster after a single dash: GNU accepts
   [-0r] for [-0 -r]. Only switches that never take an operand may cluster, so a
   cluster can never swallow the following word. *)
let xargs_cluster_switches = [ '0'; 'o'; 'p'; 'r'; 't'; 'x' ]

let is_xargs_cluster value =
  let length = String.length value in
  length > 2
  && value.[0] = '-'
  && value.[1] <> '-'
  && String.for_all
       (fun c -> List.mem c xargs_cluster_switches)
       (String.sub value 1 (length - 1))

let rec xargs_command ~grouped replacement words =
  if grouped && Option.is_some replacement then Error wrapper_syntax
  else
    match words with
    | w :: _ when not w.fixed -> Error computed_program
    | w :: rest when String.equal w.value "--" -> Ok (replacement, rest)
    | w :: token :: rest
      when String.equal w.value "-I" && token.fixed && token.value <> "" ->
        xargs_command ~grouped (Some token.value) rest
    | w :: rest when is_option w -> (
        match replacement_of w.value with
        | Some "" -> Error wrapper_syntax
        | Some token -> xargs_command ~grouped (Some token) rest
        | None -> (
            let grouped = grouped || xargs_grouping w.value in
            match
              ( option_use ~switches:xargs_switches ~arguments:xargs_arguments w.value,
                rest )
            with
            | Flag, _ -> xargs_command ~grouped replacement rest
            | Operand, value :: rest when value.fixed ->
                xargs_command ~grouped replacement rest
            | _ when is_xargs_cluster w.value -> xargs_command ~grouped replacement rest
            | _ -> Error wrapper_syntax))
    | words -> Ok (replacement, words)

let watch_switches =
  [
    "-b";
    "--beep";
    "-c";
    "--color";
    "-C";
    "--no-color";
    "-d";
    "--differences";
    "-e";
    "--errexit";
    "-g";
    "--chgexit";
    "-p";
    "--precise";
    "-r";
    "--no-rerun";
    "-t";
    "--no-title";
    "-w";
    "--no-wrap";
  ]

let watch_arguments = [ "-n"; "--interval"; "-q"; "--equexit" ]

(* procps [watch] takes an optional attached operand for differences:
   [--differences=permanent]. The attached form can never swallow the following
   word, so it leaves the command boundary unchanged. *)
let watch_optional = [ "--differences" ]

let rec watch_command direct = function
  | w :: _ when not w.fixed -> Error computed_program
  | w :: rest when String.equal w.value "--" -> Ok (direct, rest)
  | w :: rest when List.mem w.value [ "-x"; "--exec" ] -> watch_command true rest
  | w :: rest when is_option w -> (
      match
        ( option_use ~switches:watch_switches ~arguments:watch_arguments
            ~optional:watch_optional w.value,
          rest )
      with
      | Flag, _ -> watch_command direct rest
      | Operand, value :: rest when value.fixed -> watch_command direct rest
      | _ -> Error wrapper_syntax)
  | words -> Ok (direct, words)

let code_flag_operand flags w =
  if List.mem w.value flags then None
  else
    List.find_map
      (fun flag ->
        let attached =
          if String.starts_with ~prefix:"--" flag then flag ^ "=" else flag
        in
        if
          String.length w.value > String.length attached
          && String.starts_with ~prefix:attached w.value
        then Some (suffix_from w.value (String.length attached))
        else None)
      flags

let takes_code_operand flags w =
  let value = w.value in
  let length = String.length value in
  let ends_short_cluster flag =
    String.length flag = 2
    && length > 2
    && value.[0] = '-'
    && value.[1] <> '-'
    && value.[length - 1] = flag.[1]
  in
  List.mem value flags || List.exists ends_short_cluster flags

let check_executable sink base program args =
  if Option.is_none sink.refused_executable then
    if List.mem program refused_programs then sink.refused_executable <- Some program
    else if program <> base then
      let matches = function
        | first :: rest when first = program ->
            has_sequence (fun word -> basename word.value) rest args
        | _ -> false
      in
      sink.refused_executable <-
        List.find_opt matches refused_sequences |> Option.map (String.concat " ")

let rec analyze sink ~depth = function
  | [] -> ()
  | name :: args ->
      if not name.fixed then review sink computed_program
      else
        let base = basename name.value in
        let program = strip_executable_suffix base in
        check_executable sink base program args;
        analyze_program sink ~depth program args

and analyze_program sink ~depth program args =
  if List.mem program posix_shells || List.mem program foreign_shells then
    shell_code sink ~depth program args
  else if List.mem program code_runners then review sink sourced_code
  else if String.equal program "trap" then trap_action sink ~depth args
  else if String.equal program "xargs" then xargs sink ~depth args
  else if String.equal program "watch" then watch sink ~depth args
  else if List.mem program wrappers then wrapped sink ~depth program args
  else
    match List.assoc_opt program exec_flags with
    | Some flags -> exec_commands sink ~depth program flags args
    | None -> ()

and wrapped sink ~depth program args =
  match wrapped_command program args with
  | Ok command -> analyze sink ~depth command
  | Error reason ->
      review sink reason;
      (if program = "env" then
         match env_options args with
         | Error _ -> ()
         | Ok words ->
             let rec assignments = function
               | word :: rest
                 when word.assignment || (word.fixed && String.contains word.value '=') ->
                   assignments rest
               | command -> analyze sink ~depth command
             in
             assignments words);
      Option.iter
        (fun flags -> code_strings sink ~depth flags args)
        (List.assoc_opt program code_flags)

and code_strings sink ~depth flags = function
  | [] -> ()
  | w :: rest -> (
      match (code_flag_operand flags w, rest) with
      | Some code, _ ->
          if w.fixed then parse_source sink ~depth:(depth + 1) code
          else review sink computed_code;
          code_strings sink ~depth flags rest
      | None, code :: rest when takes_code_operand flags w ->
          if w.fixed && code.fixed then parse_source sink ~depth:(depth + 1) code.value
          else review sink computed_code;
          code_strings sink ~depth flags rest
      | None, _ -> code_strings sink ~depth flags rest)

and xargs sink ~depth args =
  match xargs_command ~grouped:false None args with
  | Error reason -> review sink reason
  | Ok (_, []) -> ()
  | Ok (None, command) ->
      let input =
        {
          value = "";
          fixed = false;
          unquoted = false;
          assignment = false;
          single = false;
        }
      in
      analyze sink ~depth (command @ [ input ])
  | Ok (Some token, command) ->
      let command =
        List.map
          (fun word ->
            if contains_substring ~needle:token word.value then
              { word with fixed = false; assignment = false; single = false }
            else word)
          command
      in
      analyze sink ~depth command

and watch sink ~depth args =
  match watch_command false args with
  | Error reason -> review sink reason
  | Ok (_, []) -> ()
  | Ok (true, words) -> analyze sink ~depth words
  | Ok (false, words) ->
      if List.exists (fun w -> not w.fixed) words then review sink computed_code
      else
        parse_source sink ~depth:(depth + 1)
          (String.concat " " (List.map (fun w -> w.value) words))

(* A computed scalar is safe in a known data operand, but can become an execution
   option elsewhere. Dynamic command templates can also change their terminator. *)
and exec_commands sink ~depth program flags args =
  let data_flags =
    if program = "find" then
      [
        "-name";
        "-iname";
        "-path";
        "-ipath";
        "-wholename";
        "-iwholename";
        "-regex";
        "-iregex";
        "-maxdepth";
        "-mindepth";
        "-type";
        "-xtype";
        "-newer";
        "-anewer";
        "-cnewer";
        "-samefile";
        "-amin";
        "-atime";
        "-cmin";
        "-ctime";
        "-mmin";
        "-mtime";
        "-used";
        "-size";
        "-inum";
        "-links";
        "-perm";
        "-user";
        "-group";
        "-uid";
        "-gid";
        "-lname";
        "-ilname";
        "-fstype";
        "-context";
        "-regextype";
        "-files0-from";
        "-fprint";
        "-fprint0";
        "-fls";
        "-printf";
        "-D";
      ]
    else fd_data_flags
  in
  let switches =
    if program = "find" then
      [
        "-H";
        "-L";
        "-P";
        "-O0";
        "-O1";
        "-O2";
        "-O3";
        "-daystart";
        "-follow";
        "-warn";
        "-nowarn";
        "-noleaf";
        "-ignore_readdir_race";
        "-noignore_readdir_race";
        "-depth";
        "-xdev";
        "-mount";
        "-print";
        "-print0";
        "-ls";
        "-prune";
        "-quit";
        "-delete";
        "-true";
        "-false";
        "-empty";
        "-readable";
        "-writable";
        "-executable";
        "-nogroup";
        "-nouser";
        "-a";
        "-and";
        "-o";
        "-or";
        "-not";
        "--help";
        "--version";
      ]
    else
      [
        "-H";
        "--hidden";
        "-I";
        "--no-ignore";
        "-L";
        "--follow";
        "-s";
        "--case-sensitive";
        "-i";
        "--ignore-case";
        "-F";
        "--fixed-strings";
        "-g";
        "--glob";
        "-a";
        "--absolute-path";
        "-l";
        "--list-details";
        "-0";
        "--print0";
        "-q";
        "--quiet";
        "--help";
        "--version";
      ]
  in
  let rec command_words ~batch reversed = function
    | [] -> (List.rev reversed, [])
    | word :: rest
      when word.fixed
           && (word.value = ";"
              || batch && word.value = "+"
                 &&
                 match reversed with
                 | previous :: _ -> previous.fixed && previous.value = "{}"
                 | [] -> false) ->
        (List.rev reversed, rest)
    | word :: rest -> command_words ~batch (word :: reversed) rest
  in
  let actions = ref [] in
  let action flag words =
    match List.find_opt (fun (start, _) -> start == words) !actions with
    | Some (_, rest) -> rest
    | None ->
        let batch = program = "find" && List.mem flag [ "-exec"; "-execdir" ] in
        let command, rest = command_words ~batch [] words in
        actions := (words, rest) :: !actions;
        if List.exists (fun word -> not word.fixed) command then
          review sink computed_program;
        (match command with
        | name :: _ when String.contains name.value '{' -> review sink computed_program
        | _ -> analyze sink ~depth command);
        rest
  in
  let rec ambiguous = function
    | [] -> ()
    | word :: _ when program = "fd" && word.fixed && word.value = "--" -> ()
    | word :: rest when word.fixed && List.mem word.value flags ->
        ambiguous (action word.value rest)
    | _ :: rest -> ambiguous rest
  in
  let rec scan ~ambiguous_checked = function
    | [] -> ()
    | flag :: file :: format :: rest
      when program = "find" && flag.fixed && flag.value = "-fprintf" ->
        if not (file.single && format.single) then review sink computed_program;
        scan ~ambiguous_checked rest
    | word :: rest when program = "fd" && word.fixed && fd_attached_data word.value ->
        scan ~ambiguous_checked rest
    | flag :: operand :: rest when flag.fixed && List.mem flag.value data_flags ->
        if not operand.single then review sink computed_program;
        scan ~ambiguous_checked rest
    | word :: rest when program = "fd" && word.fixed && word.value = "--" ->
        if List.exists (fun word -> not word.single) rest then
          review sink computed_program
    | word :: rest when word.fixed && List.mem word.value flags ->
        scan ~ambiguous_checked (action word.value rest)
    | word :: rest when word.fixed && is_option word ->
        if List.mem word.value switches then scan ~ambiguous_checked rest
        else (
          review sink wrapper_syntax;
          if not ambiguous_checked then ambiguous rest;
          scan ~ambiguous_checked:true rest)
    | word :: rest ->
        let fixed_path_prefix =
          List.exists
            (fun prefix -> String.starts_with ~prefix word.value)
            [ "/"; "./"; "../" ]
        in
        if (not word.fixed) && not (word.single && fixed_path_prefix) then
          review sink computed_program;
        scan ~ambiguous_checked rest
  in
  scan ~ambiguous_checked:false args

and shell_code sink ~depth program args =
  let finish ~command_string =
    review sink (if command_string then computed_code else stdin_code)
  in
  let rec options ~command_string ~from_stdin = function
    | [] -> finish ~command_string
    | w :: _ when not w.fixed -> review sink computed_code
    | w :: rest when String.equal w.value "--" -> operand ~command_string ~from_stdin rest
    | w :: _ when String.equal w.value "-" -> review sink stdin_code
    | w :: rest when String.starts_with ~prefix:"--" w.value -> (
        match (w.value, rest) with
        | ("--rcfile" | "--init-file"), _ :: rest ->
            options ~command_string ~from_stdin rest
        | _ -> options ~command_string ~from_stdin rest)
    | w :: rest when is_option w ->
        let flags = w.value in
        let command_string =
          command_string || (flags.[0] = '-' && String.contains flags 'c')
        in
        let from_stdin = from_stdin || String.contains flags 's' in
        let rest =
          match rest with
          | _ :: after when String.contains flags 'o' || String.contains flags 'O' ->
              after
          | _ -> rest
        in
        options ~command_string ~from_stdin rest
    | words -> operand ~command_string ~from_stdin words
  and operand ~command_string ~from_stdin = function
    | [] -> finish ~command_string
    | w :: _ ->
        if not command_string then (if from_stdin then review sink stdin_code)
        else if not w.fixed then review sink computed_code
        else if List.mem program posix_shells then
          parse_source sink ~depth:(depth + 1) w.value
        else review sink foreign_code
  in
  match args with
  | [ w ]
    when (program = "bash" || program = "rbash")
         && w.fixed
         && (w.value = "--version" || w.value = "--help") ->
      ()
  | _ -> options ~command_string:false ~from_stdin:false args

and trap_action sink ~depth args =
  let args =
    match args with w :: rest when String.equal w.value "--" -> rest | _ -> args
  in
  match args with
  | [] -> ()
  | w :: _ when not w.fixed -> review sink computed_code
  | w :: _ when is_option w -> ()
  | w :: _ -> parse_source sink ~depth:(depth + 1) w.value

let rec drain sink =
  match sink.commands with
  | [] -> ()
  | (depth, words) :: rest ->
      sink.commands <- rest;
      analyze sink ~depth words;
      drain sink

(* A word retains its end offset in the original command for nested Cmd payloads. *)
type cmd_item = Cmd_word of word * int | Cmd_group of string

type cmd_context =
  | Cmd_start
  | Cmd_options
  | Cmd_for_header of bool
  | Cmd_for_body
  | Cmd_echo
  | Cmd_arguments

type cmd_state = {
  chars : Buffer.t;
  mutable computed : bool;
  mutable started : bool;
  mutable awaiting_target : bool;
  mutable in_quotes : bool;
  mutable start : int;
  mutable segment : cmd_item list;
  mutable context : cmd_context;
}

let cmd_state () =
  {
    chars = Buffer.create 16;
    computed = false;
    started = false;
    awaiting_target = false;
    in_quotes = false;
    start = 0;
    segment = [];
    context = Cmd_start;
  }

let cmd_space = function
  | ' ' | '\t' | ',' | ';' | '=' | '\011' | '\012' -> true
  | _ -> false

let cmd_value = function Cmd_word (word, _) -> Some word | Cmd_group _ -> None

let cmd_lower word = String.lowercase_ascii word.value

let cmd_context_word context word =
  match context with
  | Cmd_echo | Cmd_arguments -> context
  | Cmd_for_header file_source ->
      Cmd_for_header (file_source || (word.fixed && cmd_lower word = "/f"))
  | Cmd_for_body ->
      if word.fixed && word.unquoted && cmd_lower word = "do" then Cmd_start
      else Cmd_arguments
  | Cmd_options ->
      if word.fixed && List.mem (cmd_lower word) [ "/c"; "/k" ] then Cmd_start
      else if word.fixed then Cmd_options
      else Cmd_arguments
  | Cmd_start ->
      if not word.fixed then Cmd_arguments
      else
        let name = drop_leading '@' (cmd_lower word) in
        let program = strip_executable_suffix (basename name) in
        if word.unquoted && name = "echo" then Cmd_echo
        else if program = "for" then Cmd_for_header false
        else if program = "cmd" then Cmd_options
        else if List.mem program [ "do"; "else" ] then Cmd_start
        else Cmd_arguments

let end_cmd_word sink st source index =
  if st.started || Buffer.length st.chars > 0 then (
    let value = Buffer.contents st.chars in
    sink.words <- value :: sink.words;
    if st.awaiting_target then st.awaiting_target <- false
    else
      let raw = String.sub source st.start (index - st.start) in
      let word =
        {
          value;
          fixed = not st.computed;
          unquoted = not (String.contains raw '"');
          assignment = false;
          single = not st.computed;
        }
      in
      st.context <- cmd_context_word st.context word;
      st.segment <- Cmd_word (word, index) :: st.segment);
  Buffer.clear st.chars;
  st.computed <- false;
  st.started <- false

let cmd_redirect sink st source index =
  if is_all_digits (Buffer.contents st.chars) && not st.computed then (
    Buffer.clear st.chars;
    st.started <- false)
  else end_cmd_word sink st source index;
  st.awaiting_target <- true;
  let length = String.length source in
  let next = index + 1 in
  let next = if next < length && source.[next] = '>' then next + 1 else next in
  if next < length && source.[next] = '&' then next + 1 else next

let cmd_quoted st c =
  match c with
  | '"' -> st.in_quotes <- false
  | '%' | '!' ->
      st.computed <- true;
      Buffer.add_char st.chars c
  | _ -> Buffer.add_char st.chars c

let cmd_for_file_mode items =
  List.exists
    (function Cmd_word (word, _) -> String.equal (cmd_lower word) "/f" | _ -> false)
    items

let cmd_data_group source start ~file_source =
  let length = String.length source in
  let rec first index =
    if index < length && cmd_space source.[index] then first (index + 1) else index
  in
  let first = first (start + 1) in
  let source_quote =
    if file_source && first < length then
      match source.[first] with ('\'' | '`') as quote -> Some (first, quote) | _ -> None
    else None
  in
  let rec loop index level quoted =
    if index >= length then None
    else
      match quoted with
      | Some quote ->
          if source.[index] = quote then loop (index + 1) level None
          else loop (index + 1) level quoted
      | None -> (
          if source_quote = Some (index, source.[index]) || source.[index] = '"' then
            loop (index + 1) level (Some source.[index])
          else
            match source.[index] with
            | '^' -> loop (index + 2) level None
            | '(' -> loop (index + 1) (level + 1) None
            | ')' ->
                if level = 1 then
                  Some (String.sub source (start + 1) (index - start - 1), index + 1)
                else loop (index + 1) (level - 1) None
            | _ -> loop (index + 1) level None)
  in
  loop (start + 1) 1 None

let cmd_for_variable value =
  let length = String.length value in
  let index = if length > 1 && value.[0] = '%' && value.[1] = '%' then 2 else 1 in
  length = index + 1
  && value.[0] = '%'
  && match value.[index] with 'a' .. 'z' | 'A' .. 'Z' -> true | _ -> false

let cmd_for_options options =
  let flag name word =
    word.fixed && word.unquoted && String.equal (cmd_lower word) name
  in
  match options with
  | [] -> true
  | [ word ] -> List.exists (fun name -> flag name word) [ "/d"; "/r"; "/l"; "/f" ]
  | [ mode; root ] when flag "/r" mode -> root.fixed
  | [ mode; keywords ] when flag "/f" mode -> keywords.fixed && not keywords.unquoted
  | [ directories; recursive ] -> flag "/d" directories && flag "/r" recursive
  | [ directories; recursive; root ] ->
      flag "/d" directories && flag "/r" recursive && root.fixed
  | _ -> false

let cmd_for_header items =
  let words = List.filter_map cmd_value items in
  if List.length words <> List.length items then None
  else
    match List.rev words with
    | keyword :: variable :: options
      when keyword.fixed && keyword.unquoted
           && String.equal (cmd_lower keyword) "in"
           && cmd_for_variable variable.value ->
        let options = List.rev options in
        if cmd_for_options options then Some options else None
    | _ -> None

let cmd_for_data sink source =
  let chars = Buffer.create (String.length source) in
  let rec loop index quoted =
    if index < String.length source then
      match source.[index] with
      | '"' -> loop (index + 1) (not quoted)
      | '^' when (not quoted) && index + 1 < String.length source ->
          Buffer.add_char chars source.[index + 1];
          loop (index + 2) quoted
      | ('%' | '!') as c ->
          review sink computed_code;
          Buffer.add_char chars c;
          loop (index + 1) quoted
      | c ->
          Buffer.add_char chars c;
          loop (index + 1) quoted
  in
  loop 0 false;
  sink.words <- Buffer.contents chars :: sink.words

let cmd_payload source =
  let length = String.length source in
  let rec first index =
    if index < length then
      match source.[index] with
      | ' ' | '\t' | '\n' | '\r' | '\012' -> first (index + 1)
      | _ -> index
    else index
  in
  let start = first 0 in
  let source = String.sub source start (length - start) in
  if String.length source > 0 && source.[0] = '"' then
    match String.rindex_opt source '"' with
    | Some last when last > 0 ->
        String.sub source 1 (last - 1)
        ^ String.sub source (last + 1) (String.length source - last - 1)
    | _ -> source
  else source

(* Group discovery uses the same word and Echo context as command screening.
   FOR input sets remain data; a closing parenthesis still closes a command
   block even when its last command is Echo. *)
let rec cmd_group source start ~nesting ~data ~file_source =
  if data then cmd_data_group source start ~file_source
  else if nesting >= max_nesting then None
  else
    let ignored =
      { words = []; commands = []; reviews = []; refused_executable = None }
    in
    let st = cmd_state () in
    let discard index =
      end_cmd_word ignored st source index;
      st.segment <- [];
      st.awaiting_target <- false;
      st.context <- Cmd_start;
      false
    in
    let rec loop index =
      if index >= String.length source then None
      else if st.in_quotes then (
        cmd_quoted st source.[index];
        loop (index + 1))
      else if source.[index] = ')' then
        Some (String.sub source (start + 1) (index - start - 1), index + 1)
      else
        let next, _ =
          cmd_char ignored ~nesting:(nesting + 1) ~finish:discard st source index
        in
        st.segment <- [];
        ignored.words <- [];
        ignored.reviews <- [];
        loop next
    in
    loop (start + 1)

and cmd_char sink ~nesting ~finish st source index =
  let begin_word () =
    if not st.started then (
      st.start <- index;
      st.started <- true)
  in
  match source.[index] with
  | '"' ->
      begin_word ();
      st.in_quotes <- true;
      (index + 1, false)
  | '^' ->
      begin_word ();
      (if index + 1 < String.length source then
         match source.[index + 1] with
         | '\n' | '\r' -> ()
         | next -> Buffer.add_char st.chars next);
      (min (index + 2) (String.length source), false)
  | ('%' | '!') as c ->
      begin_word ();
      st.computed <- true;
      Buffer.add_char st.chars c;
      (index + 1, false)
  | c when cmd_space c ->
      end_cmd_word sink st source index;
      (index + 1, false)
  | '(' ->
      end_cmd_word sink st source index;
      if st.context = Cmd_echo then (
        begin_word ();
        Buffer.add_char st.chars '(';
        (index + 1, false))
      else (
        let data, file_source =
          match st.context with
          | Cmd_for_header file_source -> (true, file_source)
          | _ -> (false, false)
        in
        match cmd_group source index ~nesting ~data ~file_source with
        | None ->
            review sink (unreadable "an unclosed Cmd group");
            (String.length source, false)
        | Some (group, next) ->
            st.segment <- Cmd_group group :: st.segment;
            if data then st.context <- Cmd_for_body;
            (next, false))
  | ')' when st.context = Cmd_echo ->
      begin_word ();
      Buffer.add_char st.chars ')';
      (index + 1, false)
  | ')' ->
      review sink (unreadable "an unmatched Cmd parenthesis");
      (index + 1, finish index)
  | '&' | '|' | '\n' | '\r' -> (index + 1, finish index)
  | '<' | '>' -> (cmd_redirect sink st source index, false)
  | c ->
      begin_word ();
      Buffer.add_char st.chars c;
      (index + 1, false)

let rec parse_cmd sink ~depth source =
  if depth > max_depth then (
    review sink too_deep;
    false)
  else
    let st = cmd_state () in
    let length = String.length source in
    let rec loop index any =
      if index >= length then (
        if st.in_quotes then review sink (unreadable "an unclosed Cmd quote");
        let found = end_segment sink ~depth st source length in
        any || found)
      else if st.in_quotes then (
        cmd_quoted st source.[index];
        loop (index + 1) any)
      else
        let next, found =
          cmd_char sink ~nesting:0 ~finish:(end_segment sink ~depth st source) st source index
        in
        loop next (any || found)
    in
    loop 0 false

and end_segment sink ~depth st source index =
  end_cmd_word sink st source index;
  let items = List.rev st.segment in
  st.segment <- [];
  st.context <- Cmd_start;
  if st.awaiting_target then review sink (unreadable "a missing Cmd redirection target");
  st.awaiting_target <- false;
  analyze_cmd sink ~depth ~source ~end_pos:index items

and for_command sink ~depth ~source ~end_pos args =
  let rec split header = function
    | Cmd_group source :: body -> Some (List.rev header, source, body)
    | item :: rest -> split (item :: header) rest
    | [] -> None
  in
  match split [] args with
  | None ->
      review sink for_without_do;
      false
  | Some (header, set_source, body) -> (
      match cmd_for_header header with
      | None ->
          review sink (unreadable "a for loop header");
          false
      | Some options -> (
          cmd_for_data sink set_source;
          if cmd_for_file_mode header then (
            let keywords =
              List.concat_map
                (fun word ->
                  String.split_on_char ' ' (String.lowercase_ascii word.value)
                  |> List.concat_map (String.split_on_char '\t'))
                options
            in
            (* Cmd also accepts attached keywords. Keep those unparsed forms
                behind review instead of assuming the default quote mode. *)
            if
              List.exists
                (fun keyword ->
                  keyword <> "usebackq"
                  && (not (String.starts_with ~prefix:"delims=" keyword))
                  && contains_substring ~needle:"usebackq" keyword)
                keywords
            then review sink (unreadable "attached for /f options");
            let usebackq = List.mem "usebackq" keywords in
            let source = String.trim set_source in
            let quote = if usebackq then '`' else '\'' in
            if String.length source > 0 && source.[0] = quote then
              review sink for_command_source);
          match body with
          | Cmd_word (word, _) :: rest
            when word.fixed && word.unquoted && String.equal (cmd_lower word) "do" ->
              if analyze_cmd sink ~depth:(depth + 1) ~source ~end_pos rest then true
              else (
                review sink (unreadable "a for loop without a body");
                false)
          | _ ->
              review sink for_without_do;
              false))

and analyze_cmd sink ~depth ~source ~end_pos items =
  if depth > max_depth then (
    review sink too_deep;
    false)
  else
    match items with
    | [] -> false
    | Cmd_group group_source :: rest ->
        let found = parse_cmd sink ~depth:(depth + 1) group_source in
        let following = analyze_cmd sink ~depth ~source ~end_pos rest in
        found || following
    | Cmd_word (name, _) :: args ->
        if not name.fixed then (
          review sink computed_program;
          true)
        else
          let base = basename (drop_leading '@' name.value) in
          let prog = strip_executable_suffix base in
          check_executable sink base prog (List.filter_map cmd_value args);
          if String.equal prog "for" then for_command sink ~depth ~source ~end_pos args
          else if String.equal prog "cmd" then
            let rec payload = function
              | [] -> None
              | Cmd_word (word, start) :: rest
                when word.fixed && List.mem (cmd_lower word) [ "/c"; "/k" ] ->
                  Some (start, rest)
              | Cmd_word (word, _) :: rest when word.fixed -> payload rest
              | _ ->
                  review sink computed_code;
                  None
            in
            match payload args with
            | Some (start, rest) ->
                List.iter
                  (function
                    | Cmd_word (word, _) when not word.fixed -> review sink computed_code
                    | Cmd_word _ | Cmd_group _ -> ())
                  rest;
                let payload = String.sub source start (end_pos - start) in
                if not (parse_cmd sink ~depth:(depth + 1) (cmd_payload payload)) then
                  review sink (unreadable "an empty Cmd payload");
                true
            | None -> true
          else if List.mem prog [ "do"; "else" ] then
            analyze_cmd sink ~depth:(depth + 1) ~source ~end_pos args
          else (
            if List.mem prog cmd_runners then
              List.iter
                (function
                  | Cmd_word (word, _) ->
                      if word.fixed then
                        ignore (parse_cmd sink ~depth:(depth + 1) word.value)
                      else review sink computed_program
                  | Cmd_group source -> ignore (parse_cmd sink ~depth:(depth + 1) source))
                args
            else
              List.iter
                (function
                  | Cmd_group source -> ignore (parse_cmd sink ~depth:(depth + 1) source)
                  | Cmd_word _ -> ())
                args;
            true)

let screen dialect command =
  let sink = { words = []; commands = []; reviews = []; refused_executable = None } in
  (match dialect with
  | Posix ->
      parse_source sink ~depth:0 command;
      drain sink
  | Cmd -> ignore (parse_cmd sink ~depth:0 command));
  let names text = List.map basename (fragments dialect text) in
  match refused (names command) with
  | Some program -> Blocked program
  | None -> (
      match refused (List.concat_map names (List.rev sink.words)) with
      | Some program -> Blocked program
      | None -> (
          match sink.refused_executable with
          | Some program -> Blocked program
          | None -> (
              match List.rev sink.reviews with
              | reason :: _ -> Review reason
              | [] -> Clear)))

open Lwt_direct
module Artifact = Crush_core.Artifact
module Config = Crush_core.Config
module Hooks = Crush_core.Hooks
module Jobs = Crush_core.Jobs
module Mcp = Crush_core.Mcp
module Permission = Crush_core.Permission
module Skills = Crush_core.Skills
module Todos = Crush_core.Todos
module Tool = Crush_core.Tool
module Tools_shell = Crush_core.Tools_shell
module Shell_screen = Crush_core.Shell_screen

let json_field name value = Jsont.Json.mem (Jsont.Json.name name) value

let bash_value ?working_dir ?timeout_s ?(run_in_background = false) ~command ~description
    () =
  let optional name value fields =
    match value with None -> fields | Some value -> json_field name value :: fields
  in
  [
    json_field "command" (Jsont.Json.string command);
    json_field "description" (Jsont.Json.string description);
    json_field "run_in_background" (Jsont.Json.bool run_in_background);
  ]
  |> optional "working_dir" (Option.map Jsont.Json.string working_dir)
  |> optional "timeout_s" (Option.map Jsont.Json.int timeout_s)
  |> Jsont.Json.object'

let job_value id =
  Jsont.Json.object'
    [
      json_field "job_id" (Jsont.Json.string id); json_field "wait" (Jsont.Json.bool true);
    ]

let job_kill_value id = Jsont.Json.object' [ json_field "job_id" (Jsont.Json.string id) ]

(* Same shell-compatibility layer as the jobs tests: [set /p] under [<nul]
   reads EOF and reports status 1 despite printing, so the Windows fixtures
   pin the zero exit explicitly. *)
let emit_for ~win32 text =
  if win32 then "(<nul set /p x=" ^ text ^ ")&exit 0" else "printf " ^ text

let emit text = emit_for ~win32:Sys.win32 text

let both_streams_for ~win32 =
  if win32 then "(<nul set /p x=out)&(<nul set /p x=err 1>&2)&exit 0"
  else "printf out; printf err >&2"

let both_streams = both_streams_for ~win32:Sys.win32

let long_sleep seconds =
  if Sys.win32 then "ping -n " ^ string_of_int (seconds + 1) ^ " 127.0.0.1 >nul"
  else "sleep " ^ string_of_int seconds

let make_ctx ?asker sw =
  let open Lwt.Syntax in
  let clock = Charamel_os.Time.lwt in
  let cwd = Filename.get_temp_dir_name () in
  let config = Config.default in
  let permission =
    Permission.create ~config:config.Config.permissions ~yolo:true ?asker ~cwd
      ~plans_dir:(Filename.concat cwd ".crush/plans")
      ()
  in
  let hooks = Hooks.create ~config:[] ~cwd in
  let* mcp = Mcp.create ~cwd ~config in
  let artifacts = Artifact.create ~fs_root:"/" ~dir:"tmp/crush-shell-test-artifacts" in
  let jobs = Jobs.create ~sw ~artifacts in
  let+ skills = Skills.load ~fs_root:"/" ~config ~home:cwd in
  {
    Tool.clock;
    fs_root = "/";
    random = (fun length -> String.make length '\000');
    env = Sys.getenv_opt;
    cwd;
    session = "shell-test";
    call_id = "shell-call";
    config;
    permission;
    hooks;
    lsp = None;
    mcp;
    artifacts;
    jobs;
    todos = Todos.create ();
    skills;
    log_path = Filename.concat cwd "crush-shell-test.log";
    interactive = false;
    is_subagent = false;
    ask = None;
    run_subagent = None;
    read_tracker = Hashtbl.create 8;
  }

let with_ctx ?asker f =
  let open Lwt.Syntax in
  Lwt_switch.with_switch (fun sw ->
      let* ctx = make_ctx ?asker sw in
      Lwt_direct.spawn (fun () -> f ctx))

let output_or_fail = function
  | Ok output -> output
  | Error error -> Alcotest.failf "tool failed: %a" Tool.pp_error error

let test_classifier () =
  Alcotest.(check bool)
    "git status pipeline is read-only" true
    (Tools_shell.is_read_only "git status --short | head -20");
  Alcotest.(check bool)
    "find exec is mutable" false
    (Tools_shell.is_read_only "find . -exec cat {} \\;");
  List.iter
    (fun action ->
      let command = "fd -g README.md " ^ action ^ " touch changed" in
      (match Shell_screen.screen Shell_screen.Posix command with
      | Shell_screen.Clear -> ()
      | Shell_screen.Blocked _ | Shell_screen.Review _ ->
          Alcotest.failf "a literal fd action must screen Clear: %s" command);
      Alcotest.(check bool)
        ("fd " ^ action ^ " is mutable")
        false
        (Tools_shell.is_read_only command))
    [ "-x"; "-X"; "--exec"; "--exec-batch" ];
  List.iter
    (fun command ->
      Alcotest.(check bool)
        (command ^ " has only literal data operands")
        true
        (Tools_shell.is_read_only command))
    [
      "fd -- -X";
      "fd -- --exec";
      "fd -e -X README.md";
      "fd -E --exec README.md";
      "fd -t f README.md";
      "fd -tf README.md";
    ];
  Alcotest.(check bool)
    "fd option operand does not end option parsing" false
    (Tools_shell.is_read_only "fd -e -- -X touch changed");
  (match Shell_screen.screen Shell_screen.Posix "fd -tf README.md -x touch changed" with
  | Shell_screen.Clear -> ()
  | Shell_screen.Blocked _ | Shell_screen.Review _ ->
      Alcotest.fail "the preserved attached fd action must screen Clear");
  Alcotest.(check bool)
    "fd attached filter with an execution action is mutable" false
    (Tools_shell.is_read_only "fd -tf README.md -x touch changed");
  List.iter
    (fun command ->
      Alcotest.(check bool)
        (command ^ " stays read-only")
        true
        (Tools_shell.is_read_only command))
    [
      "fd -tf README";
      "fd --type=f README";
      "fd -e-X README";
      "fd --extension=--exec README";
    ];
  List.iter
    (fun command ->
      (match Shell_screen.screen Shell_screen.Posix command with
      | Shell_screen.Clear -> ()
      | Shell_screen.Blocked _ | Shell_screen.Review _ ->
          Alcotest.failf "an attached filter must retain the action: %s" command);
      Alcotest.(check bool)
        (command ^ " stays mutable") false
        (Tools_shell.is_read_only command))
    [ "fd -tf README -x printf ready"; "fd --type=f README --exec-batch printf ready" ];
  List.iter
    (fun command ->
      Alcotest.(check bool)
        (command ^ " retains its write boundary")
        false
        (Tools_shell.is_read_only command))
    [ "FLAG=value fd --type=f README"; "fd --type=f README > out" ];
  Alcotest.(check bool)
    "sed in-place is mutable" false
    (Tools_shell.is_read_only "sed -i s/a/b/ file");
  Alcotest.(check bool)
    "awk system form is mutable" false
    (Tools_shell.is_read_only "awk '{ system(\"id\") }' file");
  Alcotest.(check bool)
    "environment assignment is mutable" false
    (Tools_shell.is_read_only "FLAG=value ls")

let run_command ctx command =
  Tools_shell.bash.Tool.run ctx (bash_value ~command ~description:"screen" ())

let refused_forms =
  [
    "c''url --version";
    "cu\"\"rl --version";
    "c\\url --version";
    "'cu'\"rl\" --version";
    "/usr/bin/c''url --version";
    "sh -c \"c''url --version\"";
    "echo $(c''url --version)";
  ]

let test_screen_reads_quoting () =
  if not Sys.win32 then
    await @@ with_ctx
    @@ fun ctx ->
    let unrefused =
      List.filter
        (fun command ->
          match run_command ctx command with
          | Error (`Unavailable _) -> false
          | Ok _ | Error _ -> true)
        refused_forms
    in
    Alcotest.(check (list string)) "every quoted form is refused" [] unrefused

(* The shell computes the program each form runs, so no blanket approval may run it. *)
let computed_forms =
  [
    "$(printf ech)o reviewed";
    "X=ech; ${X}o reviewed";
    "`printf ech`o reviewed";
    "printf 'echo reviewed' | sh";
    "env $(printf ech)o reviewed";
    {|GROUP='value printf reviewed'; env GROUP=$GROUP true|};
    {|FLAG=-exec; find . "$FLAG" printf reviewed \;|};
    {|FLAG=-x; fd "$FLAG" printf reviewed|};
    ". /dev/null";
    "printf $'\\'\n$(printf ech)o reviewed\n'";
  ]

let test_screen_requires_review () =
  if not Sys.win32 then (
    await @@ with_ctx
    @@ fun ctx ->
    let unreviewed =
      List.filter
        (fun command ->
          match run_command ctx command with
          | Error (`Denied _) -> false
          | Ok _ | Error _ -> true)
        computed_forms
    in
    Alcotest.(check (list string)) "yolo does not run a computed program" [] unreviewed;
    await
    @@ with_ctx ~asker:(fun _ -> Permission.Allow_once)
    @@ fun ctx ->
    let output = output_or_fail (run_command ctx "$(printf ech)o reviewed") in
    Alcotest.(check bool)
      "an explicit approval runs it" true
      (Test_support.contains ~needle:"reviewed" ~haystack:output.Tool.content))

let check_posix_verdicts cases =
  let mismatches =
    List.filter_map
      (fun (command, expected) ->
        let actual =
          match Shell_screen.screen Shell_screen.Posix command with
          | Shell_screen.Clear -> "clear"
          | Shell_screen.Blocked _ -> "blocked"
          | Shell_screen.Review _ -> "review"
        in
        if actual = expected then None else Some (command, (expected, actual)))
      cases
  in
  Alcotest.(check (list (pair string (pair string string))))
    "classifier mismatches (command, (expected, actual))" [] mismatches

let test_heredoc_boundaries () =
  check_posix_verdicts
    [
      ("cat <<$TAG\ntext\n$TAG\n(", "review");
      ("cat <<$TAG\ntext\n$TAG\nprintf done", "clear");
      ("cat <<TAG\nmissing terminator", "review");
      ("cat <<-\"$TAG\"\n\ttext\n\t$TAG", "clear");
      (* An escaped backslash pair is a literal backslash, not a continuation. *)
      ("cat <<EOF\nfoo\\\\\nbar\nEOF", "clear");
      (* A continued delimiter joins before the delimiter check, so commands
         after the real body end are still screened. *)
      ("cat <<EOF\nEO\\\nF\nc''url https://example.com\nEOF", "blocked");
      ("cat <<-EOF\nEO\\\n\tF\nc''url https://example.com\nEOF", "review");
    ]

let test_command_lookup () =
  check_posix_verdicts
    [
      ("command -v git", "clear");
      ("command -V git", "clear");
      ("command -p -v git", "clear");
      ("command -pv git", "clear");
      ("command -pV git", "clear");
      ("command git status", "clear");
      ("command curl https://example.com", "blocked");
      ("command -vx git", "review");
    ]

let test_ansi_c_nul () =
  check_posix_verdicts
    [
      ({|$'\x63url' --version|}, "blocked");
      ({|printf '%s' $'before\0after'|}, "review");
      ({|printf '%s' $'before\x00after'|}, "review");
      ({|printf '%s' $'before\u0000after'|}, "review");
    ]

let test_parameter_quotes () =
  check_posix_verdicts
    [
      ({|printf '%s' "${status_message:-it's ready}"|}, "clear");
      ({|printf '%s' ${status_message:-'some text'}|}, "clear");
      ({|printf '%s' ${status_message:-it's ready}|}, "review");
    ]

let ordinary_forms =
  [
    ("cd \"$(dirname /tmp/x)\" && printf ok", "ok");
    ("for word in a b; do printf \"$word\"; done", "ab");
    ("printf '%s' \"$(printf nested)\"", "nested");
    ("sh -c 'printf inner'", "inner");
    ("cat <<'EOF'\nheredoc $(not run)\nEOF", "heredoc $(not run)");
  ]

let test_screen_clears_ordinary () =
  if not Sys.win32 then
    await @@ with_ctx
    @@ fun ctx ->
    List.iter
      (fun (command, expected) ->
        let output = output_or_fail (run_command ctx command) in
        Alcotest.(check bool)
          command true
          (Test_support.contains ~needle:expected ~haystack:output.Tool.content))
      ordinary_forms

let test_read_only_wiring () =
  await @@ with_ctx
  @@ fun ctx ->
  let requests = ref [] in
  let permission =
    Permission.create ~config:Config.default.permissions ~yolo:false ~cwd:ctx.Tool.cwd
      ~plans_dir:(Permission.plans_dir ctx.permission)
      ~hook:(fun request ->
        requests := request :: !requests;
        `Deny "permission wiring test")
      ()
  in
  let ctx = { ctx with Tool.permission } in
  let cases =
    [
      ("ls | head -20", false, true);
      ("ls | head -20", true, false);
      ("mkdir fixture", false, false);
    ]
  in
  List.iter
    (fun (command, run_in_background, _) ->
      let result =
        Tools_shell.bash.Tool.run ctx
          (bash_value ~command ~description:"read-only wiring" ~run_in_background ())
      in
      match result with
      | Error (`Denied "permission wiring test") -> ()
      | _ -> Alcotest.fail "the request did not stop at the policy hook")
    cases;
  Alcotest.(check (list (pair string bool)))
    "read-only reaches policy only for the foreground read"
    (List.map (fun (command, _, read_only) -> (command, read_only)) cases)
    (List.rev_map
       (fun (request : Permission.request) ->
         (request.Permission.action, request.Permission.read_only))
       !requests)

let screen_table =
  let posix command expected = (Shell_screen.Posix, command, expected) in
  let cmd command expected = (Shell_screen.Cmd, command, expected) in
  [
    posix "git status --short | head -20" `Clear;
    posix "cd \"$(git rev-parse --show-toplevel)\" && dune build" `Clear;
    posix "for f in *.ml; do ocamlformat \"$f\"; done" `Clear;
    posix "case \"$1\" in a) echo a;; *) echo b;; esac" `Clear;
    posix "FOO=$(date) make test" `Clear;
    posix "sh script.sh" `Clear;
    posix "xargs -I {} cp {} /tmp" `Clear;
    posix "find . -name '*.ml' -exec grep -l x {} +" `Clear;
    posix "curl https://example.com" `Blocked;
    posix "cu\\\nrl x" `Blocked;
    posix "$'\\143url' x" `Blocked;
    posix "$'\\u0063url' x" `Blocked;
    posix "bash -c 'c\"\"url x'" `Blocked;
    posix "timeout 5s sh -c \"c''url x\"" `Blocked;
    posix "watch \"c''url x\"" `Blocked;
    posix "x=$(c''url x)" `Blocked;
    posix "cat <<EOF\n$(c''url x)\nEOF" `Blocked;
    posix "/usr/local/bin/brew install x" `Blocked;
    posix "$CMD x" `Review;
    posix "\"$CMD\" x" `Review;
    posix "c*l x" `Review;
    posix "c{u,}rl x" `Review;
    posix "{fd}>/dev/null $CMD" `Review;
    posix "true &>/dev/null $CMD" `Review;
    posix "(( $CMD ))" `Review;
    posix "[[ -n x || $CMD ]]" `Review;
    posix "printf $'\\'\n$CMD\n'" `Review;
    posix "xargs -I{} {} x" `Review;
    posix "find /usr/bin -name x -exec {} \\;" `Review;
    posix "env -S \"$CMD x\"" `Review;
    posix "fish -c 'echo hi'" `Review;
    posix "echo 'unterminated" `Review;
    posix
      (String.concat "" (List.init 5000 (fun _ -> "$(")) ^ "x" ^ String.make 5000 ')')
      `Review;
    posix "script -qc \"c''url\" /dev/null" `Blocked;
    posix "echo hi | xargs -I % % x" `Review;
    posix "cat <<EOF | sh\necho hi\nEOF" `Review;
    cmd "cmd /c echo ready for review" `Clear;
    cmd "cmd /c for %i in (*)" `Review;
    cmd "cmd /c \"echo ready for review\"" `Clear;
    cmd "cmd /d /c echo do for else" `Clear;
    cmd "for /r do %i in (*.txt) do echo %i" `Clear;
    cmd "for /r \"do\" %i in (*.txt) do echo %i" `Clear;
    cmd "for %i in (do) do echo %i" `Clear;
    cmd "for %i in (alpha) do echo ready for review" `Clear;
    cmd "for /f %i in (notes.txt) do echo %i" `Clear;
    cmd "for /f \"tokens=2\" %i in (notes.txt) do echo %i" `Clear;
    cmd "for /f %i in (\"literal words\") do echo %i" `Clear;
    cmd "for /f \"usebackq\" %i in (\"notes space.txt\") do echo %i" `Clear;
    cmd "for /f \"usebackq\" %i in ('literal words') do echo %i" `Clear;
    cmd "for /f \"usebackq\" %i in (`echo alpha`) do echo %i" `Review;
    cmd "for /f \"tokens=* usebackq\" %i in (`echo alpha`) do echo %i" `Review;
    cmd "for /f \"delims=usebackq\" %i in ('echo alpha') do echo %i" `Review;
    (* Frozen native option/quote observations. Attached keywords are
       intentionally unparsed and therefore require exact-command approval. *)
    cmd "for /f %i in ('echo 9173') do @echo RESULT:[%i]" `Review;
    cmd "for /f %i in (`echo 9173`) do @echo RESULT:[%i]" `Clear;
    cmd "for /f \"usebackq\" %i in ('echo 9173') do @echo RESULT:[%i]" `Clear;
    cmd "for /f \"usebackq\" %i in (`echo 9173`) do @echo RESULT:[%i]" `Review;
    cmd "for /f \"delims=usebackq\" %i in ('echo 9173') do @echo RESULT:[%i]" `Review;
    cmd "for /f \"delims=usebackq\" %i in (`echo 9173`) do @echo RESULT:[%i]" `Clear;
    cmd "for /f \"delims= usebackq\" %i in ('echo 9173') do @echo RESULT:[%i]" `Clear;
    cmd "for /f \"delims= usebackq\" %i in (`echo 9173`) do @echo RESULT:[%i]" `Review;
    cmd "for /f \"tokens=* delims= usebackq\" %i in ('echo 9173') do @echo RESULT:[%i]"
      `Clear;
    cmd "for /f \"tokens=* delims= usebackq\" %i in (`echo 9173`) do @echo RESULT:[%i]"
      `Review;
    cmd "for /f \"usebackqdelims=\" %i in ('echo 9173') do @echo RESULT:[%i]" `Review;
    cmd "for /f \"usebackqdelims=\" %i in (`echo 9173`) do @echo RESULT:[%i]" `Review;
    cmd "for /f \"tokens=*usebackq\" %i in ('echo 9173') do @echo RESULT:[%i]" `Review;
    cmd "for /f \"tokens=*usebackq\" %i in (`echo 9173`) do @echo RESULT:[%i]" `Review;
    cmd "for /f \"usebackq tokens=* delims=\" %i in ('echo 9173') do @echo RESULT:[%i]"
      `Clear;
    cmd "for /f \"usebackq tokens=* delims=\" %i in (`echo 9173`) do @echo RESULT:[%i]"
      `Review;
    cmd "for /f \"delims=usebackq tokens=*\" %i in ('echo 9173') do @echo RESULT:[%i]"
      `Review;
    cmd "for /f \"delims=usebackq tokens=*\" %i in (`echo 9173`) do @echo RESULT:[%i]"
      `Clear;
    cmd "for %i in (alpha) do" `Review;
    cmd "for %i in (alpha) do   " `Review;
    cmd "for /r do %i in (*.txt) do" `Review;
    cmd "for %i in (do) do" `Review;
    cmd "for %i in (alpha) do ()" `Review;
    cmd "for %i in (alpha) do (())" `Review;
    cmd "for %i in (alpha) do & echo after" `Review;
    cmd "for %i in (alpha) & for %j in (beta) do echo %j" `Review;
    cmd "for %i in (alpha) do echo %i & for %j in (beta)" `Review;
    cmd "cmd /c for %i in (alpha)" `Review;
    cmd "cmd /c \"for %i in (alpha) do\"" `Review;
    cmd "for %i in (alpha do echo ready" `Review;
    cmd "for in (alpha) do echo ready" `Review;
    cmd "for %i (alpha) do echo ready" `Review;
    cmd "for unsupported %i in (alpha) do echo ready" `Review;
    cmd "for %i in (alpha) do %PROGRAM% %i" `Review;
    cmd "cmd /c %PROGRAM% ready" `Review;
    cmd "cmd /c echo %TEXT%" `Review;
    cmd "for %i in (alpha) do for %j in (beta) do echo %i %j" `Clear;
    cmd "for %i in (alpha) do (echo %i)" `Clear;
    cmd "for %i in (alpha) do (echo %i & echo ready for review)" `Clear;
    cmd "for %i in (c^url) do echo %i" `Blocked;
    cmd "for %i in (alpha) do c^url x" `Blocked;
    cmd "for %i in (alpha) do %PROGRAM% & c^url x" `Blocked;
    cmd "for %f in (a.txt b.txt) do type \"%f\"" `Clear;
    cmd "for %%f in (a.txt b.txt) do type \"%%f\"" `Clear;
    cmd "for /l %i in (1,1,10) do @echo hi" `Clear;
    cmd "for %f in (a.txt b.txt)" `Review;
    cmd "for %f in (%FILES%) do type \"%f\"" `Review;
    cmd "for /f \"tokens=*\" %i in ('dir') do echo %i" `Review;
    cmd "c^url x" `Blocked;
    cmd "cu\"\"rl x" `Blocked;
    cmd "C:\\Windows\\System32\\curl.exe x" `Blocked;
    cmd "cmd /c \"c^url x\"" `Blocked;
    cmd "%CMD% x" `Review;
    cmd "echo(" `Clear;
    cmd "@echo(" `Clear;
    cmd "EcHo(" `Clear;
    cmd "echo(ready" `Clear;
    cmd "echo((ready" `Clear;
    cmd "echo(ready)" `Clear;
    cmd "echo (ready" `Clear;
    cmd "echo \"(\"" `Clear;
    cmd "echo ^(" `Clear;
    cmd "echo(ready & echo(after" `Clear;
    cmd "echo literal(for)" `Clear;
    cmd "(echo( & echo ready)" `Clear;
    cmd "(echo(ready)" `Clear;
    cmd "(echo foo(bar))" `Review;
    cmd "(echo foo(bar^))" `Clear;
    cmd "for %i in (alpha beta) do @echo(" `Clear;
    cmd "for %i in (alpha) do (echo( & echo %i)" `Clear;
    cmd "(echo ready)" `Clear;
    cmd "for %i in (alpha) do @echo %i" `Clear;
    cmd "(for %i in (alpha) do @echo %i)" `Clear;
    cmd "for %i in (alpha)" `Review;
    cmd "(echo ready" `Review;
    cmd "cmd /d /c echo(" `Clear;
    cmd "cmd /d /c \"echo(\"" `Clear;
    cmd "cmd /d /c cmd /d /c echo(" `Clear;
    cmd "cmd /d /c echo key=value" `Clear;
    cmd "cmd /d /c echo key value" `Clear;
    cmd "cmd /d /c echo first,,second;;third=last" `Clear;
    cmd "cmd /d /c echo first second third last" `Clear;
    cmd "cmd /d /c echo key\tvalue" `Clear;
    cmd "cmd /d /c echo key value" `Clear;
    cmd "cmd /d /c \"echo key=value\"" `Clear;
    cmd "cmd /d /c cmd /d /c echo key=value" `Clear;
    cmd "cmd /d /c for /f \"delims==\" %i in (\"key=value\") do @echo %i" `Review;
    cmd "cmd /d /c echo key=value>capture.txt" `Clear;
    cmd "cmd /d /c 2>capture.txt echo key=value" `Clear;
    cmd "cmd /d /c =,;echo key=value" `Clear;
    cmd "cmd /d /c for %i in (alpha) do @echo(" `Review;
    cmd "echo(^&" `Clear;
    cmd "echo(ready & (echo after)" `Clear;
    cmd "cmd /d /c echo key=value  " `Clear;
    cmd "cmd /d /c echo key=value" `Clear;
    cmd "cmd /d /c   echo key=value" `Clear;
    cmd "cmd /d /c \"echo key=value\"  " `Clear;
    cmd "cmd /d /c \"echo key=value  \"" `Clear;
    cmd "cmd /d /c =,;echo key=value  " `Clear;
    cmd "echo( & %COMSPEC% /d /c echo reviewed" `Review;
    cmd "(echo( & %COMSPEC% /d /c echo reviewed)" `Review;
    cmd "for %i in (alpha) do echo( & %COMSPEC% /d /c echo reviewed" `Review;
    cmd "cmd /c echo( & %COMSPEC% /d /c echo reviewed" `Review;
    cmd "echo( & curl.exe" `Blocked;
    cmd "(echo( & curl.exe)" `Blocked;
    cmd
      "(((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((echo()))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))"
      `Review;
  ]

let test_screen_table () =
  let kind = function
    | Shell_screen.Clear -> `Clear
    | Shell_screen.Blocked _ -> `Blocked
    | Shell_screen.Review _ -> `Review
  in
  let mismatched =
    List.filter_map
      (fun (dialect, command, expected) ->
        if kind (Shell_screen.screen dialect command) = expected then None
        else Some command)
      screen_table
  in
  Alcotest.(check (list string)) "every command gets its expected verdict" [] mismatched

let cmd_payload_cases =
  [
    ("nested_equal", "cmd /d /c echo key=value", "clear", [ "cmd /d /c echo key=value"; "echo key=value" ]);
    ("nested_equal_spaced", "cmd /d /c echo key value", "clear", [ "cmd /d /c echo key value"; "echo key value" ]);
    ("nested_punctuation", "cmd /d /c echo first,,second;;third=last", "clear", [ "cmd /d /c echo first,,second;;third=last"; "echo first,,second;;third=last" ]);
    ("nested_punctuation_spaced", "cmd /d /c echo first second third last", "clear", [ "cmd /d /c echo first second third last"; "echo first second third last" ]);
    ("nested_tab", "cmd /d /c echo key\tvalue", "clear", [ "cmd /d /c echo key\tvalue"; "echo key\tvalue" ]);
    ("nested_tab_spaced", "cmd /d /c echo key value", "clear", [ "cmd /d /c echo key value"; "echo key value" ]);
    ("nested_blank", "cmd /d /c echo(", "clear", [ "cmd /d /c echo("; "echo(" ]);
    ("nested_quoted_blank", "cmd /d /c \"echo(\"", "clear", [ "cmd /d /c \"echo(\""; "echo(" ]);
    ("nested_double_blank", "cmd /d /c cmd /d /c echo(", "clear", [ "cmd /d /c cmd /d /c echo("; "cmd /d /c echo("; "echo(" ]);
    ("nested_quoted_equal", "cmd /d /c \"echo key=value\"", "clear", [ "cmd /d /c \"echo key=value\""; "echo key=value" ]);
    ("nested_double_equal", "cmd /d /c cmd /d /c echo key=value", "clear", [ "cmd /d /c cmd /d /c echo key=value"; "cmd /d /c echo key=value"; "echo key=value" ]);
    ("nested_for_equal", "cmd /d /c for /f \"delims==\" %i in (\"key=value\") do @echo %i", "review", [ "cmd /d /c for /f \"delims==\" %i in (\"key=value\") do @echo %i"; "for /f \"delims==\" %i in (\"key=value\") do @echo %i" ]);
    ("nested_redirect_equal", "cmd /d /c echo key=value>capture.txt", "clear", [ "cmd /d /c echo key=value>capture.txt"; "echo key=value" ]);
    ("nested_redirect_prefix", "cmd /d /c 2>capture.txt echo key=value", "clear", [ "cmd /d /c 2>capture.txt echo key=value"; "echo key=value" ]);
    ("nested_prefix_separators", "cmd /d /c =,;echo key=value", "clear", [ "cmd /d /c =,;echo key=value"; "=,;echo key=value" ]);
    ("nested_loop_blank", "cmd /d /c for %i in (alpha) do @echo(", "review", [ "cmd /d /c for %i in (alpha) do @echo("; "for %i in (alpha) do @echo(" ]);
    ("trailing_plain", "cmd /d /c echo key=value  ", "clear", [ "cmd /d /c echo key=value  "; "echo key=value  " ]);
    ("trailing_trimmed_control", "cmd /d /c echo key=value", "clear", [ "cmd /d /c echo key=value"; "echo key=value" ]);
    ("leading_plain", "cmd /d /c   echo key=value", "clear", [ "cmd /d /c   echo key=value"; "echo key=value" ]);
    ("quoted_trailing_outside", "cmd /d /c \"echo key=value\"  ", "clear", [ "cmd /d /c \"echo key=value\"  "; "echo key=value  " ]);
    ("quoted_trailing_inside", "cmd /d /c \"echo key=value  \"", "clear", [ "cmd /d /c \"echo key=value  \""; "echo key=value  " ]);
    ("prefix_trailing", "cmd /d /c =,;echo key=value  ", "clear", [ "cmd /d /c =,;echo key=value  "; "=,;echo key=value  " ]);
    ("for-body-source-offset", "for %i in (alpha) do cmd /d /c echo key=value", "clear", [ "for %i in (alpha) do cmd /d /c echo key=value"; "echo key=value" ]);
    ("group-local-source-offset", "(cmd /d /c echo key=value)", "clear", [ "(cmd /d /c echo key=value)"; "cmd /d /c echo key=value"; "echo key=value" ]);
    ("reported_quoted_outer", "cmd /d /c \"echo ready\" > \"capture (sales & support).txt\"", "clear", [ "cmd /d /c \"echo ready\" > \"capture (sales & support).txt\""; "echo ready " ]);
    ("quoted_outer_stderr", "cmd /d /c \"echo ready\" 2> \"capture (sales & support).txt\"", "clear", [ "cmd /d /c \"echo ready\" 2> \"capture (sales & support).txt\""; "echo ready " ]);
    ("quoted_outer_both", "cmd /d /c \"echo ready\" > \"capture (sales & support).txt\" 2>&1", "clear", [ "cmd /d /c \"echo ready\" > \"capture (sales & support).txt\" 2>&1"; "echo ready " ]);
    ("quoted_outer_append", "cmd /d /c \"echo ready\" >> \"capture (sales & support).txt\"", "clear", [ "cmd /d /c \"echo ready\" >> \"capture (sales & support).txt\""; "echo ready " ]);
    ("redirect_before_payload", "cmd /d /c > \"capture (sales & support).txt\" \"echo ready\"", "clear", [ "cmd /d /c > \"capture (sales & support).txt\" \"echo ready\""; "echo ready" ]);
    ("redirect_before_switch", "cmd > \"capture (sales & support).txt\" /d /c \"echo ready\"", "clear", [ "cmd > \"capture (sales & support).txt\" /d /c \"echo ready\""; "echo ready" ]);
    ("quoted_inner", "cmd /d /c \"echo ready>capture.txt\"", "clear", [ "cmd /d /c \"echo ready>capture.txt\""; "echo ready>capture.txt" ]);
    ("inner_plus_outer", "cmd /d /c \"echo ready>capture.txt\" > \"capture (sales & support).txt\"", "clear", [ "cmd /d /c \"echo ready>capture.txt\" > \"capture (sales & support).txt\""; "echo ready>capture.txt " ]);
    ("quoted_literal_greater", "cmd /d /c echo \"ready>text\"", "clear", [ "cmd /d /c echo \"ready>text\""; "echo \"ready>text\"" ]);
    ("two_outer_targets", "cmd /d /c \"echo ready\" > \"capture (sales & support).txt\" 2>error.txt", "clear", [ "cmd /d /c \"echo ready\" > \"capture (sales & support).txt\" 2>error.txt"; "echo ready " ]);
    ("following_command", "cmd /d /c \"echo ready\" > \"capture (sales & support).txt\" & echo done", "clear", [ "cmd /d /c \"echo ready\" > \"capture (sales & support).txt\" & echo done"; "echo ready  " ]);
    ("grouped_outer", "(cmd /d /c \"echo ready\" > \"capture (sales & support).txt\")", "clear", [ "(cmd /d /c \"echo ready\" > \"capture (sales & support).txt\")"; "cmd /d /c \"echo ready\" > \"capture (sales & support).txt\""; "echo ready " ]);
    ("for_body_outer", "for %I in (1) do cmd /d /c \"echo ready\" > \"capture (sales & support).txt\"", "clear", [ "for %I in (1) do cmd /d /c \"echo ready\" > \"capture (sales & support).txt\""; "echo ready " ]);
    ("trailing_space_outer", "cmd /d /c \"echo ready\"  > \"capture (sales & support).txt\"  ", "clear", [ "cmd /d /c \"echo ready\"  > \"capture (sales & support).txt\"  "; "echo ready    " ]);
    ("echo_two_spaces", "cmd /d /c echo ready >capture.txt  2>\"capture (sales & support).txt\"", "clear", [ "cmd /d /c echo ready >capture.txt  2>\"capture (sales & support).txt\""; "echo ready " ]);
    ("echo_tab_gap", "cmd /d /c echo ready >capture.txt\t2>\"capture (sales & support).txt\"", "clear", [ "cmd /d /c echo ready >capture.txt\t2>\"capture (sales & support).txt\""; "echo ready " ]);
    ("echo_argument_between", "cmd /d /c echo ready >capture.txt tail >\"capture (sales & support).txt\"", "clear", [ "cmd /d /c echo ready >capture.txt tail >\"capture (sales & support).txt\""; "echo ready  tail " ]);
    ("echo_duplication_before", "cmd /d /c echo ready 2>&1 >\"capture (sales & support).txt\"", "clear", [ "cmd /d /c echo ready 2>&1 >\"capture (sales & support).txt\""; "echo ready " ]);
    ("echo_comma_gap", "cmd /d /c echo ready >capture.txt , 2>\"capture (sales & support).txt\"", "clear", [ "cmd /d /c echo ready >capture.txt , 2>\"capture (sales & support).txt\""; "echo ready " ]);
    ("echo_quoted_digit", "cmd /d /c echo \"2\">capture.txt", "clear", [ "cmd /d /c echo \"2\">capture.txt"; "echo \"2\"" ]);
    ("echo_quoted_digits", "cmd /d /c echo \"12\">capture.txt", "clear", [ "cmd /d /c echo \"12\">capture.txt"; "echo \"12\"" ]);
    ("echo_two_digits", "cmd /d /c echo 12>capture.txt", "clear", [ "cmd /d /c echo 12>capture.txt"; "echo 12" ]);
    ("echo_escaped_digit", "cmd /d /c echo ^2>capture.txt", "clear", [ "cmd /d /c echo ^2>capture.txt"; "echo ^2" ]);
    ("echo_ordinary_digit", "cmd /d /c echo 2>capture.txt", "clear", [ "cmd /d /c echo 2>capture.txt"; "echo " ]);
  ]

let test_cmd_payload_sources () =
  let kind = function
    | Shell_screen.Clear -> "clear"
    | Shell_screen.Blocked _ -> "blocked"
    | Shell_screen.Review _ -> "review"
  in
  List.iter
    (fun (label, command, expected_kind, expected_sources) ->
      let verdict, sources = Shell_screen.For_test.screen_cmd command in
      Alcotest.(check (list string))
        ("exact Cmd parser inputs: " ^ label) expected_sources sources;
      Alcotest.(check string)
        ("Cmd verdict: " ^ label) expected_kind (kind verdict);
      Alcotest.(check bool)
        ("observation keeps the public verdict: " ^ label)
        true (verdict = Shell_screen.screen Shell_screen.Cmd command))
    cmd_payload_cases

let test_wrapper_operands () =
  check_posix_verdicts
    [
      ("env -u UNUSED sh", "review");
      ("env --unset UNUSED sh", "review");
      ("timeout 1 sh", "review");
      ({|timeout -- "$duration" printf ready|}, "review");
      ({|timeout 1 dune build "$target"|}, "clear");
      ({|xargs -n 1 printf '%s' "$label"|}, "clear");
      ({|env printf -S "$label"|}, "clear");
      ("env --unmodelled option sh", "review");
      (* A quoted expansion in an assignment prefix provably forms one
         argument, but a computed word elsewhere still needs approval. *)
      ({|env GROUP="$group" printenv GROUP|}, "clear");
      ({|env GROUP=$group printf ready|}, "review");
      ({|env GROUP=${group} printf ready|}, "review");
      ({|env GROUP=$group ./cu''rl.exe --version|}, "blocked");
      ({|env GROUP=$group ./brew.exe install fixture|}, "blocked");
      ({|env - GROUP=$group ./brew.exe install fixture|}, "blocked");
      ({|env -u UNUSED - ./brew.exe install fixture|}, "blocked");
      ({|env GROUP=value - ./brew.exe install fixture|}, "clear");
      ({|env "$target" printenv|}, "review");
      ({|env GROUP="${@}" printf ready|}, "review");
      (* Clusters of no-operand xargs switches and the attached optional
         watch operand leave the command boundary unchanged. *)
      ({|printf 'a' | xargs -0r printf '%s'|}, "clear");
      ("watch --differences=permanent -x printf sample", "clear");
      ("xargs -0I{} sh -c 'echo hi'", "review");
      (* A literal [flock -c] command string is parsed but still needs an
         explicit approval. *)
      ("flock lockfile -c 'printf ready'", "review");
    ]

let test_review_argument_boundaries () =
  check_posix_verdicts
    [
      ("find . -name \"${pattern:-{@*.txt}\" -print", "clear");
      ("find . -name \"${pattern:-{!CHAR32_PREFIX@*.txt}\" -print", "clear");
      ("find . -name \"${pattern:-literal@text}\" -print", "clear");
      ("env GROUP=value - ./brew.exe install fixture", "clear");
      ("env -u UNUSED GROUP=value - ./brew.exe install fixture", "clear");
      ("env -u UNUSED - ./brew.exe install fixture", "blocked");
      ("env - GROUP=value ./brew.exe install fixture", "blocked");
      ("env GROUP=value -i ./brew.exe install fixture", "clear");
      ("env A=\"${@}\" true", "review");
      ("env A=\"${@:1}\" true", "review");
      ("env A=\"${@:-fallback}\" true", "review");
      ("env A=\"${!prefix@}\" true", "review");
      ("find . -name \"${@}\" -print", "review");
      ("find . -name \"${@:1}\" -print", "review");
      ("find . -name \"${@:-fallback}\" -print", "review");
      ("find . -name \"${!prefix@}\" -print", "review");
      ("find . -name \"${value:-literal@text}\" -print", "clear");
      ("find . -name \"${value@Q}\" -print", "clear");
      ("find . -name \"${!prefix*}\" -print", "clear");
      ("fd -tf README", "clear");
      ("fd --type=f README", "clear");
      ("fd -e.rs README", "clear");
      ("fd --extension=rs README", "clear");
      ("fd -Etarget README", "clear");
      ("fd --exclude=target README", "clear");
      ("fd -d2 README", "clear");
      ("fd --max-depth=2 README", "clear");
      ("fd -tf README -x printf '%s'", "clear");
      ("fd --type=f README --exec-batch printf '%s'", "clear");
      ("fd -e-X README", "clear");
      ("fd --extension=--exec README", "clear");
      ("fd --typefuture=f README", "review");
      ("fd -t\"$kind\" README", "review");
      ("fd -tf README -x \"$PROGRAM\"", "review");
      ("fd -tf README -x ./cu''rl.exe", "blocked");
      ("command -v \"$NAME\"", "clear");
      ("command -pv git", "clear");
      ("command -pV git", "clear");
      ("command -vp git", "clear");
      ("command -p -v git", "clear");
      ("command -pp printf ready", "clear");
      ("command -pv -p git", "clear");
      ("command -pv -- git", "clear");
      ("command -pz git", "review");
      ("command -v -z git", "review");
      ("command -- -pv", "clear");
      ("command -p ./cu''rl.exe", "blocked");
      ("command -pv ./cu''rl.exe", "clear");
      ("env -i - ./cu''rl.exe", "blocked");
      ("env -u UNUSED - ./cu''rl.exe", "blocked");
      ("env -i -- - ./cu''rl.exe", "blocked");
      ("env - -i ./cu''rl.exe", "clear");
      ("env -i - A=\"$@\" ./cu''rl.exe", "blocked");
      ("env -i - \"$PROGRAM\"", "review");
      ("env -i - A=one printf ready", "clear");
    ]

let test_find_data_slots () =
  check_posix_verdicts
    [
      ("fd -g README.md", "clear");
      ("fd --glob README.md", "clear");
      ("fd --type f README.md", "clear");
      ("fd -t f README.md", "clear");
      ("fd -tf README.md", "clear");
      ("fd --type=f README.md", "clear");
      ("find -O0 . -print", "clear");
      ("find -O1 . -print", "clear");
      ("find -O2 . -print", "clear");
      ("find -O3 . -print", "clear");
      ("fd --color never -- -x ./links.com", "review");
      ("fd --future-option -- -X ./links.com", "review");
      ({|find -O3 . -name -exec -exec ./cu''rl.exe '{}' \;|}, "blocked");
      ({|find . -future-option -name -exec -exec ./cu''rl.exe '{}' \;|}, "blocked");
      ( {|find . -future-option -fprintf out -exec -execdir ./cu''rl.exe '{}' \;|},
        "blocked" );
      ({|find . -name "$pattern" -print|}, "clear");
      ({|find . -maxdepth "$depth" -mindepth "$minimum" -type "$kind"|}, "clear");
      ({|find . -maxdepth $depth -type "$kind"|}, "review");
      ({|find . -newer -name -exec sh -c "$command" \;|}, "review");
      ({|find . -newer -name -exec ./cu''rl.exe \;|}, "blocked");
      ({|find . -future-option -name -exec ./cu''rl.exe \;|}, "blocked");
      ({|find . -ok printf '{}' + -exec ./cu''rl.exe \;|}, "clear");
      ({|find "$dir" -name '*.ml' -print|}, "review");
      ({|find "./$dir" -name '*.ml' -print|}, "clear");
      ("find . -name $pattern -print", "review");
      ("find $dir -print", "review");
      ({|find . -name "$@" -print|}, "review");
      ({|find . -name "${@}" -print|}, "review");
      ({|find . -name "${@:1}" -print|}, "review");
      ({|find . -name "${!prefix@}" -print|}, "review");
      ({|find . "$flag" sh -c 'printf executed' \;|}, "review");
      ({|fd "$flag" sh -c 'printf executed'|}, "review");
      ({|fd -- "$pattern"|}, "clear");
      ({|fd -e "$extension" -X printf '%s'|}, "clear");
      ({|fd -tf README.md -X printf '%s'|}, "clear");
      ({|find . -name '-exec' -print|}, "clear");
      ({|find . -exec printf '%s' '{}' \; -exec curl '{}' \;|}, "blocked");
      ({|find . -exec printf '%s' "$label" \;|}, "review");
    ]

let test_nested_fd_actions () =
  let command = String.concat "" (List.init 24 (fun _ -> "fd --color never -X ")) in
  check_posix_verdicts
    [ (command ^ "printf ready", "review"); (command ^ "./cu''rl.exe", "blocked") ]

let test_ansi_c_bash_equivalence () =
  if not Sys.win32 then (
    if Option.is_none (Charamel_os.Exe.find "bash") then Alcotest.skip ();
    await
    @@ with_ctx ~asker:(fun _ -> Permission.Allow_once)
    @@ fun ctx ->
    (* Bash truncates an argument at an ANSI-C NUL escape; the screen models
       that truncation, so the suffix never reaches the command. *)
    let output =
      output_or_fail (run_command ctx {|bash -c "printf '%s' \$'before\\0after'"|})
    in
    Alcotest.(check bool)
      "bash truncates at NUL" true
      (Test_support.contains ~needle:"before" ~haystack:output.Tool.content);
    Alcotest.(check bool)
      "bash drops the NUL suffix" false
      (Test_support.contains ~needle:"after" ~haystack:output.Tool.content))

let test_xargs_replacement () =
  check_posix_verdicts
    [
      ("xargs -I SLOT sh -c 'printf %s SLOT'", "review");
      ("xargs -I SLOT printf '%s' SLOT", "clear");
      ("xargs sh -c", "review");
      ("xargs -I SLOT -n 2 printf '%s' SLOT", "review");
    ]

let test_executable_suffix () =
  check_posix_verdicts
    [
      ("file ./curl.exe", "clear");
      ("file ./cu''rl.exe", "clear");
      ("./curl.exe --version", "blocked");
      ("./brew.exe install fixture", "blocked");
      ("env ./cu''rl.exe --version", "blocked");
    ]

let test_shell_information () =
  check_posix_verdicts
    [
      ("bash --version", "clear");
      ("bash --help", "clear");
      ("bash --noprofile", "review");
      ("sh --help", "review");
    ]

let test_watch_mode () =
  check_posix_verdicts
    [
      ("watch -x grep -F '(' app.log", "clear");
      ("watch --interval 2 --exec -- grep -F '(' app.log", "clear");
      ("watch --equexit 2 grep -F '(' app.log", "review");
    ]

let test_cmd_loop_permissions () =
  if Sys.win32 then (
    let command_source = "for /f %i in ('echo reviewed') do @echo %i" in
    await @@ with_ctx
    @@ fun ctx ->
    List.iter
      (fun command ->
        match run_command ctx command with
        | Error (`Denied _) -> ()
        | Ok _ | Error _ -> Alcotest.failf "blanket approval ran %s" command)
      [
        command_source;
        "for %i in (alpha) do";
        "for /f \"usebackqdelims=\" %i in (`echo 9173`) do @echo RESULT:[%i]";
        "for /f \"tokens=*usebackq\" %i in (`echo 9173`) do @echo RESULT:[%i]";
      ];
    let ordinary =
      output_or_fail (run_command ctx "for /f %i in (\"literal words\") do @echo %i")
    in
    Alcotest.(check bool)
      "a literal source needs no review" true
      (Test_support.contains ~needle:"literal" ~haystack:ordinary.Tool.content);
    await
    @@ with_ctx ~asker:(fun _ -> Permission.Allow_once)
    @@ fun ctx ->
    let output = output_or_fail (run_command ctx command_source) in
    Alcotest.(check bool)
      "an exact approval permits the command source" true
      (Test_support.contains ~needle:"reviewed" ~haystack:output.Tool.content);
    List.iter
      (fun command ->
        let output = output_or_fail (run_command ctx command) in
        Alcotest.(check bool)
          "an exact approval permits attached command-source options" true
          (Test_support.contains ~needle:"RESULT:[9173]" ~haystack:output.Tool.content))
      [
        "for /f \"usebackqdelims=\" %i in (`echo 9173`) do @echo RESULT:[%i]";
        "for /f \"tokens=*usebackq\" %i in (`echo 9173`) do @echo RESULT:[%i]";
      ])

let test_cmd_echo_permissions () =
  if Sys.win32 then (
    await @@ with_ctx
    @@ fun ctx ->
    List.iter
      (fun command ->
        let output = output_or_fail (run_command ctx command) in
        Alcotest.(check bool)
          ("literal echo exits: " ^ command)
          false output.Tool.is_error)
      [
        "echo(";
        "@echo(";
        "cmd /d /c echo(";
        "(echo( & echo ready)";
        "for %i in (alpha beta) do @echo(";
      ];
    List.iter
      (fun command ->
        match run_command ctx command with
        | Error (`Denied _) -> ()
        | Ok _ | Error _ -> Alcotest.failf "blanket approval ran %s" command)
      [
        "echo( & %COMSPEC% /d /c echo reviewed"; "(echo( & %COMSPEC% /d /c echo reviewed)";
      ])

let test_cmd_redirection_permissions () =
  if Sys.win32 then (
    await @@ with_ctx
    @@ fun ctx ->
    let capture = Filename.temp_file "capture (sales & support)" ".txt" in
    Fun.protect
      ~finally:(fun () -> Sys.remove capture)
      (fun () ->
        let command = Printf.sprintf "cmd /d /c \"echo ready\" > \"%s\"" capture in
        let output = output_or_fail (run_command ctx command) in
        Alcotest.(check bool) "literal redirection exits" false output.Tool.is_error;
        let text = In_channel.with_open_bin capture In_channel.input_all in
        Alcotest.(check string)
          "outer redirection keeps the native trailing space" "ready \r\n" text))

let test_foreground_capture () =
  await @@ with_ctx
  @@ fun ctx ->
  let input = bash_value ~command:both_streams ~description:"capture both streams" () in
  let output = output_or_fail (Tools_shell.bash.Tool.run ctx input) in
  Alcotest.(check bool)
    "stdout is captured" true
    (Test_support.contains ~needle:"out" ~haystack:output.Tool.content);
  Alcotest.(check bool)
    "stderr is captured" true
    (Test_support.contains ~needle:"err" ~haystack:output.Tool.content);
  Alcotest.(check bool) "successful process is not an error" false output.Tool.is_error

let test_timeout () =
  await @@ with_ctx
  @@ fun ctx ->
  let input =
    bash_value ~timeout_s:1 ~command:(long_sleep 5) ~description:"deadline" ()
  in
  match Tools_shell.bash.Tool.run ctx input with
  | Error (`Timeout seconds) -> Alcotest.(check (float 1e-9)) "deadline" 1. seconds
  | Error error -> Alcotest.failf "unexpected timeout result: %a" Tool.pp_error error
  | Ok _ -> Alcotest.fail "sleep exceeded its deadline"

let test_background_lifecycle () =
  await @@ with_ctx
  @@ fun ctx ->
  let started =
    output_or_fail
      (Tools_shell.bash.Tool.run ctx
         (bash_value ~timeout_s:5 ~run_in_background:true ~command:(emit "background")
            ~description:"background output" ()))
  in
  let prefix = "started " in
  Alcotest.(check bool)
    "job id is returned" true
    (String.starts_with ~prefix started.Tool.content);
  let id =
    String.sub started.Tool.content (String.length prefix)
      (String.length started.Tool.content - String.length prefix)
  in
  let output = output_or_fail (Tools_shell.job_output.Tool.run ctx (job_value id)) in
  Alcotest.(check bool)
    "background stdout is retained" true
    (Test_support.contains ~needle:"background" ~haystack:output.Tool.content);
  let killed_started =
    output_or_fail
      (Tools_shell.bash.Tool.run ctx
         (bash_value ~timeout_s:5 ~run_in_background:true ~command:(long_sleep 30)
            ~description:"background kill" ()))
  in
  let killed_id =
    String.sub killed_started.Tool.content (String.length prefix)
      (String.length killed_started.Tool.content - String.length prefix)
  in
  ignore (output_or_fail (Tools_shell.job_kill.Tool.run ctx (job_kill_value killed_id)));
  let killed_output =
    output_or_fail (Tools_shell.job_output.Tool.run ctx (job_value killed_id))
  in
  Alcotest.(check bool)
    "job kill reports killed" true
    (Test_support.contains ~needle:"killed" ~haystack:killed_output.Tool.content)

(* Linux cannot execute the [cmd /c] fixtures, so this pins their shape: the
   Windows fixtures must end with an explicit zero exit. *)
let windows_fixtures_pin_zero_exit () =
  Alcotest.(check bool)
    "windows emit ends with an explicit zero exit" true
    (String.ends_with ~suffix:"&exit 0" (emit_for ~win32:true "probe"));
  Alcotest.(check bool)
    "windows both_streams ends with an explicit zero exit" true
    (String.ends_with ~suffix:"&exit 0" (both_streams_for ~win32:true))

let cases =
  [
    Test_tools_test_support.case "conservative shell classifier" `Quick test_classifier;
    Test_tools_test_support.case "screen reads shell quoting" `Quick
      test_screen_reads_quoting;
    Test_tools_test_support.case "literal here-document boundaries" `Quick
      test_heredoc_boundaries;
    Test_tools_test_support.case "ANSI-C NUL screening" `Quick test_ansi_c_nul;
    Test_tools_test_support.case "parameter quote context" `Quick test_parameter_quotes;
    Test_tools_test_support.case "command lookup mode" `Quick test_command_lookup;
    Test_tools_test_support.case "find data positions" `Quick test_find_data_slots;
    Test_tools_test_support.case "nested fd actions are screened once" `Quick
      test_nested_fd_actions;
    Test_tools_test_support.case "ANSI-C NUL matches bash" `Quick
      test_ansi_c_bash_equivalence;
    Test_tools_test_support.case "screen requires review" `Quick
      test_screen_requires_review;
    Test_tools_test_support.case "screen clears ordinary commands" `Quick
      test_screen_clears_ordinary;
    Test_tools_test_support.case "screen verdict table" `Quick test_screen_table;
    Test_tools_test_support.case "review argument boundaries" `Quick
      test_review_argument_boundaries;
    Test_tools_test_support.case "Cmd loop approval boundary" `Quick
      test_cmd_loop_permissions;
    Test_tools_test_support.case "Cmd echo approval boundary" `Quick
      test_cmd_echo_permissions;
    Test_tools_test_support.case "Cmd payload source bytes" `Quick
      test_cmd_payload_sources;
    Test_tools_test_support.case "Cmd redirection approval boundary" `Quick
      test_cmd_redirection_permissions;
    Test_tools_test_support.case "wrapper option and argument boundaries" `Quick
      test_wrapper_operands;
    Test_tools_test_support.case "xargs replacement positions" `Quick
      test_xargs_replacement;
    Test_tools_test_support.case "executable suffix positions" `Quick
      test_executable_suffix;
    Test_tools_test_support.case "shell information switches" `Quick
      test_shell_information;
    Test_tools_test_support.case "watch execution mode" `Quick test_watch_mode;
    Test_tools_test_support.case "foreground capture" `Quick test_foreground_capture;
    Test_tools_test_support.case "foreground deadline" `Quick test_timeout;
    Test_tools_test_support.case "background lifecycle" `Quick test_background_lifecycle;
    Test_tools_test_support.case "permission receives read-only classification" `Quick
      test_read_only_wiring;
    Test_tools_test_support.case "windows fixtures pin zero exit" `Quick
      windows_fixtures_pin_zero_exit;
  ]

(** Static screen for shell tool commands.

    The screen reads a command the way the shell that runs it will. It removes quotes,
    joins the pieces of a word, decodes backslash and [$'...'] escapes, and reads the code
    inside command substitutions, backquotes, here-documents, [sh -c] strings, [trap]
    actions, and the commands that common wrappers ([env], [xargs], [find -exec], [watch],
    ...) start. When [$'...'] appears it reads the command twice, once as bash and once as
    a shell without that quoting (dash), because the two split such a command into
    different words. Ambiguous wrapper options and command operands require review. A
    computed [find] or [fd] argument needs review unless a literal option makes it a
    single data operand, or a fixed path prefix rules out an option. An unquoted [env]
    assignment that can split into more arguments also needs review.

    The list of refused programs is a hint, not a boundary: a program that runs its own
    arguments as code (an interpreter, a script, a build tool) is outside any shell-level
    screen, so the permission prompt stays the boundary for every command. The screen
    makes one guarantee: a command whose program the shell computes at run time is never
    reported as {!Clear}. *)

type dialect =
  | Posix  (** The command runs as [/bin/sh -c command]. *)
  | Cmd  (** The command runs as [cmd /c command]. *)

type verdict =
  | Clear
      (** Every program the shell starts is named literally and none is on the refused
          list. *)
  | Blocked of string
      (** After quote removal the command names a refused program or a refused install
          form; the payload names it. The shell tool refuses such a command whatever the
          permission policy says. *)
  | Review of string
      (** The shell computes the name of a program it starts, runs code the screen cannot
          read, or cannot parse the command. Only an explicit approval of this exact
          command may run it; the payload says why. *)

val platform : dialect
(** [platform] is {!Cmd} on Windows and {!Posix} elsewhere. *)

val screen : dialect -> string -> verdict
(** [screen dialect command] classifies [command]. {!Blocked} takes precedence over
    {!Review}, and a command that fails to parse is {!Review} unless it is {!Blocked}. *)

val fd_executes : string list -> bool
(** [fd_executes args] recognizes execution actions in literal [fd] arguments that have
    already passed {!screen} as {!Clear}. Known data operands and arguments after [--] are
    not options. The argument list excludes the program name. *)


module For_test : sig
  val screen_cmd : string -> verdict * string list
  (** [screen_cmd command] returns the same verdict as [screen Cmd command] and
      the exact source strings submitted to the Cmd parser in traversal order,
      starting with [command]. This is an observation of the screen's recursive
      inputs, not a record of commands executed by a native shell. *)
end

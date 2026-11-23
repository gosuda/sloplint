namespace Case0
structure R where
  x : Nat
  y : Nat

def pair : R := { x := 1, y := 2 }

end Case0

namespace Case1
structure R where
  x : Nat
  y : Nat

def pair := { x := 1, y := 2 : R }

end Case1

namespace Case2
structure R where
  x : Nat
  y : Nat

def pair : R := { x := 1, y := 2, }

end Case2

namespace Case3
structure R where
  x : Nat
  y : Nat

def pair : R := {
  x := 1
  y := 2
}

end Case3

namespace Case4
structure R where
  x : Nat
  y : Nat

def pair : R :=
  { x := 1
    y := 2 }

end Case4

namespace Case5
structure R where
  x : Nat
  y : Nat

def pair (base : R) : R :=
  { base with x := 1
              y := 2 }

end Case5

namespace Case6
structure R where
  x : Nat
  y : Nat

def pair (base : R) : R :=
  { base with
    x := 1
    y := 2 }

end Case6

namespace Case7
structure R where
  x : Nat
  y : Nat

def pair : R := { x : Nat := 1, y : Nat := 2 }

end Case7

namespace Case8
structure F where
  run : Nat → Nat

def value : F := { run (x : Nat) : Nat := x + 1 }

end Case8

namespace Case9
def action (flag : Bool) : Id Unit := do
  if flag then
    pure ()
  pure ()

end Case9

namespace Case10
def action (flag : Bool) : Id Unit := do
  if flag then pure ()
  pure ()

end Case10

namespace Case11
def action (flag : Bool) : Id Nat := do
  let mut n := 0
  if flag then
    n := 1
  return n

end Case11

namespace Case12
def action (flag : Bool) : Id Nat := do
  if flag then
    let n := 1
    return n
  else
    let n := 2
    return n

end Case12

namespace Case13
def action (value : Option Nat) : Id Nat := do
  if let some n := value then
    return n
  return 0

end Case13

namespace TacticSequences

theorem nested (h : True) : True := by
  have inner : True := by
    simpa only using h
  have outer := inner
  exact outer

theorem identity (n : Nat) : n = n := by
  induction n with
  | zero =>
    rfl
  | succ n ih =>
    have copy := ih
    simpa only using congrArg Nat.succ copy

theorem conjunction (h : True ∧ True) : True := by
  rcases h with ⟨left, right⟩
  exact left

theorem pair : True ∧ True := by
  constructor
  · exact True.intro
  · exact True.intro

def fromBlock : List Nat := Id.run do
  pure [1]

def nestedFor : Id Unit := do
  for x in (Id.run do pure [1]) do
    let _ := x
    pure ()

structure ProofRecord where
  proof : True

def proofRecord : ProofRecord :=
  { proof := by
      exact True.intro }

def after : Nat := 0

end TacticSequences

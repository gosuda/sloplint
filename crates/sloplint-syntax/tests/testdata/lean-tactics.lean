namespace ResidualTactics

theorem renameEq (n : Nat) : n = n := by
  generalize hn : n = m at *
  rfl

theorem inject (n m : Nat) (h : Nat.succ n = Nat.succ m) : n = m := by
  injection h with same

theorem discardOr : (True ∨ True) → True := by
  rintro (left | right)
  all_goals assumption

theorem splitPair : True ∧ True := by
  constructor
  next => exact True.intro
  next => exact True.intro

theorem splitAgain : True ∧ True := by
  constructor
  all_goals first | exact True.intro

theorem repeated : True := by
  repeat first | exact True.intro

theorem atAll (n : Nat) (h : n + 0 = n) : n = n := by
  simp only [Nat.add_zero] at *

end ResidualTactics

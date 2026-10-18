let check_user (name : string) : unit =
  Alcotest.check Alcotest.string "user name matches" name "alice"

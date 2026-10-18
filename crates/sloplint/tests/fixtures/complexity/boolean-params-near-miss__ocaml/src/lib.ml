let render_table (header : int) (footer : string) : string =
  if header > 0 then footer else "none"

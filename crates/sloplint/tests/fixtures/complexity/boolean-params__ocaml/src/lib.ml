let render_table (header : bool) (footer : bool) : string =
  if header then (if footer then "both" else "head") else "none"

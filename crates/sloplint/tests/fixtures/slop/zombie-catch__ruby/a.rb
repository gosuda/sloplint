def load(path)
  read_file(path)
rescue IOError => e
  raise
end

def load(path)
  read_file(path)
rescue IOError => e
  puts(e.message)
  "default"
end

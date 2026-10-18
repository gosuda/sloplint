def describe(status)
  case status
  when Status::ACTIVE
    "active"
  when Status::IDLE
    "idle"
  else
    "other"
  end
end

function Convert-User {
    param($User)
    @{
        Name = $User.Name
        Email = $User.Email
        Age = $User.Age
    }
}

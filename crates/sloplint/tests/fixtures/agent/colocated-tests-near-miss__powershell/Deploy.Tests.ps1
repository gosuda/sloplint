Describe "Publish-Deploy" {
  It "returns the name" {
    Publish-Deploy "web" | Should -Be "web" -Because "deployment names round-trip"
  }
}

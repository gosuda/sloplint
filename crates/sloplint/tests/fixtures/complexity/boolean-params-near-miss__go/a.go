package probe
type Console struct{}
func (c Console) Render(verbose bool) {}
func helper(flag bool) bool {
	return !flag
}

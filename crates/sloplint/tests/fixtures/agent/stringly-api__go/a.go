package probe
func ProcessOrder(id string, account string, qty int) int {
	return len(id) + len(account) + qty
}

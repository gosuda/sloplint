typedef struct {
    int ident;
} OrderId;

typedef struct {
    int count;
} Quantity;

void Move(OrderId order, Quantity qty) {
    (void)order;
    (void)qty;
}

struct Stack {
    int inner_len() const;
    int len() const { return this->inner_len(); }
};
int place(int x, int y) { return x + y; }
int move_to(int x, int y) { return place(y, x); }
int fresh() { return 0; }

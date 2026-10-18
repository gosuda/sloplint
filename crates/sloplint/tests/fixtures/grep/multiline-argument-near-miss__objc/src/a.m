int Twice(int x) {
    return x * 2;
}

int RunAll(int data) {
    int r = Apply(data, ^{
        int a = 1;
        int b = 2;
        return a + b;
    });
    return Twice(r);
}

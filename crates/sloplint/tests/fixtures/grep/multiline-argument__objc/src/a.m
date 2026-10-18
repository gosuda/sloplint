int Twice(int x) {
    return x * 2;
}

int RunAll(int data) {
    int r = Apply(data, ^{
        int a = 1;
        int b = 2;
        int c = 3;
        int d = 4;
        int e = 5;
        return a + b + c + d + e;
    });
    return Twice(r);
}

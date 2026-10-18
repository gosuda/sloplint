public class Converter {
    public Point Convert(Raw p) {
        return new Point { X = p.X, Y = p.Y, Z = p.Z, W = p.W + 1 };
    }
}

public class Point {
    public int X { get; set; }
    public int Y { get; set; }
    public int Z { get; set; }
    public int W { get; set; }
}

public class Raw {
    public int X { get; set; }
    public int Y { get; set; }
    public int Z { get; set; }
    public int W { get; set; }
}

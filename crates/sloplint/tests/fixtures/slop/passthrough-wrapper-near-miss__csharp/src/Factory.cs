public class Factory {
    public Widget Create() {
        return BuildWidget();
    }

    private Widget BuildWidget() {
        return new Widget();
    }
}

public class Widget {
}

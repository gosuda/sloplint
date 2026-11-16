void handle() {}
void process() {}

class Widget {
    Q_OBJECT
    void handle() {}
public:
    void process() {}
};

class UThing {
    GENERATED_BODY()
    void handle() {}
public:
    void process() {}
};

class ULegacy {
    GENERATED_UCLASS_BODY()
    void handle() {}
};

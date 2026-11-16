struct Gadget {
    Q_GADGET
    void process() {}
};

class Object : public QObject {
public:
    Q_OBJECT
    void handle() {}
};

struct DeclaredGadget {
    Q_GADGET
    void manage();
};
void DeclaredGadget::manage() {}

class DeclaredObject : public QObject {
public:
    Q_OBJECT
    void perform();
};
void DeclaredObject::perform() {}

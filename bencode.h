typedef struct {
    char *str;
    int len;
} String;

typedef struct Node {
    void *data;
    struct Node *next;
    char type;
} Node;

typedef struct {
    Node *head;
    Node *tail;
} List;

typedef struct {
    char *key;
    void *val;
} Pair;

void append(List *list, void *data);

List *parse(char *str);

void print_tree(List *tree);

void free_ll(List *tree);
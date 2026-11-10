// The few libstdc++ functions NVIDIA's NGX loader (libnvsdk_ngx.a, built
// with GCC) calls, for the portable Linux release, which links LLVM's
// libc++ instead so it runs on older systems. Each follows libstdc++'s
// behavior for the layout its inline code already assumes; the names are
// libstdc++'s mangled ones, so nothing here clashes with libc++.
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <new>
#include <stdexcept>
#include <system_error>

namespace {

// std::__cxx11::basic_string<T>: pointer, length, then a 16-byte union of
// the short-string buffer and the heap capacity.
template <class T>
struct String {
    T* data;
    std::size_t length;
    union {
        T local[16 / sizeof(T)];
        std::size_t capacity;
    };
};

template <class T>
T* create(std::size_t& capacity, std::size_t old) {
    const std::size_t max = (SIZE_MAX / sizeof(T) - 1) / 2;
    if (capacity > max) {
        throw std::length_error("basic_string::_M_create");
    }
    if (capacity > old && capacity < 2 * old) {
        capacity = 2 * old < max ? 2 * old : max;
    }
    return static_cast<T*>(::operator new((capacity + 1) * sizeof(T)));
}

}  // namespace

extern "C" {

char* ty_string_create(void*, std::size_t* capacity, std::size_t old) __asm__(
    "_ZNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEE9_M_createERmm");
char* ty_string_create(void*, std::size_t* capacity, std::size_t old) {
    return create<char>(*capacity, old);
}

wchar_t* ty_wstring_create(void*, std::size_t* capacity, std::size_t old) __asm__(
    "_ZNSt7__cxx1112basic_stringIwSt11char_traitsIwESaIwEE9_M_createERmm");
wchar_t* ty_wstring_create(void*, std::size_t* capacity, std::size_t old) {
    return create<wchar_t>(*capacity, old);
}

void ty_string_assign(String<char>* self, const String<char>* other) __asm__(
    "_ZNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEE9_M_assignERKS4_");
void ty_string_assign(String<char>* self, const String<char>* other) {
    if (self == other) {
        return;
    }
    const std::size_t length = other->length;
    const bool local = self->data == self->local;
    const std::size_t capacity = local ? sizeof(self->local) - 1 : self->capacity;
    if (length > capacity) {
        std::size_t wanted = length;
        char* data = create<char>(wanted, capacity);
        if (!local) {
            ::operator delete(self->data);
        }
        self->data = data;
        self->capacity = wanted;
    }
    if (length) {
        std::memcpy(self->data, other->data, length);
    }
    self->length = length;
    self->data[length] = '\0';
}

// std::ios_base::Init sets up std::cout and friends, which NGX never uses.
void ty_ios_init(void*) __asm__("_ZNSt8ios_base4InitC1Ev");
void ty_ios_init(void*) {}
void ty_ios_exit(void*) __asm__("_ZNSt8ios_base4InitD1Ev");
void ty_ios_exit(void*) {}

[[noreturn]] void ty_throw_bad_alloc() __asm__("_ZSt17__throw_bad_allocv");
[[noreturn]] void ty_throw_bad_alloc() { throw std::bad_alloc(); }
[[noreturn]] void ty_throw_logic_error(const char*) __asm__("_ZSt19__throw_logic_errorPKc");
[[noreturn]] void ty_throw_logic_error(const char* what) { throw std::logic_error(what); }
[[noreturn]] void ty_throw_system_error(int) __asm__("_ZSt20__throw_system_errori");
[[noreturn]] void ty_throw_system_error(int code) {
    throw std::system_error(code, std::generic_category());
}

}  // extern "C"

# Calculator that works with 4 numbers and operations

def tambah(a, b):
    return a + b

def kurang(a, b):
    return a - b

def kali(a, b):
    return a * b

def bagi(a, b):
    if b != 0:
        return a / b
    else:
        return "Error: Tidak bisa dibagi dengan 0"

def main():
    print("=== Kalkulator 4 Angka ===")
    
    try:
        # Input 4 angka
        nums = []
        for i in range(4):
            num = float(input(f"Masukkan angka ke-{i+1}: "))
            nums.append(num)
        
        # Input 3 operator
        operators = []
        for i in range(3):
            op = input(f"Masukkan operator ke-{i+1} (+, -, *, /): ")
            while op not in ['+', '-', '*', '/']:
                print("Operator tidak valid! Masukkan +, -, *, atau /")
                op = input(f"Masukkan operator ke-{i+1} (+, -, *, /): ")
            operators.append(op)
        
        # Hitung berdasarkan urutan operasi
        hasil = nums[0]
        for i in range(3):
            if operators[i] == '+':
                hasil = tambah(hasil, nums[i+1])
            elif operators[i] == '-':
                hasil = kurang(hasil, nums[i+1])
            elif operators[i] == '*':
                hasil = kali(hasil, nums[i+1])
            elif operators[i] == '/':
                hasil = bagi(hasil, nums[i+1])
        
        # Tampilkan perhitungan
        print(f"\nPerhitungan: {nums[0]} {operators[0]} {nums[1]} {operators[1]} {nums[2]} {operators[2]} {nums[3]}")
        print(f"Hasil: {hasil}")
        
    except ValueError:
        print("Error: Masukkan angka yang valid!")

if __name__ == "__main__":
    while True:
        main()
        lagi = input("\nHitung lagi? (y/n): ")
        if lagi.lower() != 'y':
            break
    print("Terima kasih!")
# T507/T527 macOS 네이티브 플래싱 툴 — FEL/EFEX 기술조사

**작성일**: 2026-09-06
**목적**: PhoenixSuit(Windows 전용)를 대체하는 macOS 네이티브 FEL 플래싱 툴 개발을 위한 실물 SDK 기반 기술 검증
**조사 대상 SDK**: `/Volumes/jejezz/Work/AllWinnerT527/android13`

---

## 0. 요약

원 제안서(2026-09-06 작성)에서 미지수로 남겨두었던 항목들을 실제 T527 Android 13 SDK(`brandy-2.0`, U-Boot 2018 소스, 최종 fusing 이미지)를 직접 열어서 전부 실물로 검증했다. 핵심 결론:

1. **fastboot는 벽돌/공장초기 복구에 쓸 수 없다.** fastboot는 이미 정상 부팅 가능한 시스템(`WORK_MODE_BOOT`)에서만 도달 가능한 경로이고, FEL 복구 시나리오에서는 **EFEX/FES가 유일하고 필수적인 경로**다. (초기에 "fastboot가 더 유리하다"고 판단했던 것은 오판이었음 — 아래 3.3절 참조)
2. **EFEX 진입 조건은 명확하다**: U-Boot 이미지 헤더의 `work_mode` 필드(파일 절대 offset **224**)를 `WORK_MODE_USB_PRODUCT`(`0x10`)로 패치하면 된다. 이 오프셋은 실제 최종 fusing 이미지에서 바이트 단위로 검증했고, **실제 Windows PhoenixSuit 로그(11절)에서도 `workmode = 16(0x10)`으로 정확히 일치 확인**했다.
3. **패치 후 체크섬 재계산이 필요하며, 알고리즘은 SDK 내 `mksunxiboot.c`에서 확보**했다 (암호화 서명 아님, 단순 32비트 워드섬).
4. **Rust CLI(`aw-tool`)로 IMAGEWTY 파싱·work_mode 패치·체크섬·FEL USB 프로토콜을 직접 구현하여 실기(T527 "pluto_wallpad" 보드)와 라이브 통신 성공**(11절). `sunxi-fel spl fes1.fex`로 fes1을 올렸을 때 두 차례 보드 응답 불능(전원 재인가로 복구)이 있었으나, 원인은 **로드 주소 오류**(`sunxi-fel`이 가정하는 0x44000이 아니라 fes1은 실제로 0x4C000용)로 소스에서 확정했고, 올바른 주소로 raw write+exec하여 DRAM 초기화 성공까지 검증함 — 아래 11.3절.
5. **"BOOT1"/"BOOT0"의 정체를 실물 로그로 확인**: EFEX의 `SUNXI_EFEX_BOOT1_TAG`로 쓰는 것은 `boot_package.fex`(u-boot+BL31+scp+dtb TOC1 번들), `SUNXI_EFEX_BOOT0_TAG`로 쓰는 것은 `boot0_sdcard.fex`다. 이는 일반 파티션 write와는 별도의 전용 EFEX 태그로 처리된다 (11.2절).
6. **work_mode 패치는 "FEL 업로드용 프라이머"에만 적용되고, 실제로 저장소에 write되는 boot0/boot1은 원본(work_mode=0x00) 그대로 써야 한다** — 이 둘을 혼동하면 안 된다 (11.2절).
7. **최종 fusing 이미지(`t527_android13_pluto_wallpad_uart0.img`)는 IMAGEWTY 컨테이너**이며, 그 내부 아이템 테이블 포맷(1024바이트 고정 레코드, offset/length 필드 위치)을 실제 데이터로 역산·검증했다. `sys_partition.fex`를 이미지에서 직접 잘라내 원본과 바이트 단위로 일치함을 확인했다.
8. EFEX 프로토콜의 정확한 커맨드 세트는 `usb_efex.c`에서 실물로 확보했다 (리버스엔지니어링 아닌 벤더 소스 그대로).
9. **"BOOTLOADER" GPT 파티션(`boot-resource.fex`)은 SPL/BOOT0/BOOT1과 무관하다** — `pack` 스크립트 소스(`boot_resource_list` vs `boot_file_list`)와 FAT16 파일 목록 직접 파싱으로 이중 확인했다 (9절 항목 6, 13절).
10. **FEL→EFEX 진입까지 툴에 내장하여 외부 sunxi-tools 의존을 제거했다** — FEL 상태에서 `aw-tool flash-all <image> <sys_partition.fex>` 한 줄로 부트스트랩+전체 플래싱+재부팅이 1분 42초에 끝난다 (16절).
11. **`super.fex`는 raw 파티션 이미지가 아니라 Android sparse 이미지 컨테이너다** — 이걸 그대로 쓰면 파티션 선두에 liblp 메타데이터 대신 sparse 헤더가 놓여, 커널은 부팅하지만 first-stage init이 dynamic partition을 못 찾고 마운트 시도 전에 bootloader로 재부팅한다. unsparse 구현 후 **PhoenixSuit 없이 macOS 툴 단독으로 전체 플래싱 → 정상 Android 부팅 성공** (15절, 최종 해결).

---

## 1. 전체 플래싱 체인 (End-to-End)

```
① BROM (FEL 모드 대기)
     USB VID:PID = 1f3a:efe8
     완전히 빈 eMMC/벽돌 상태에서도 항상 진입 가능
        │  FEL로 fes1 업로드+실행
        ▼
② fes1_sun55iw3p1.bin  (DRAM 초기화 전용 최소 스텁, 75줄)
     UART/보드/DRAM 초기화만 하고 리턴. EFEX 프로토콜 처리 안 함.
        │  FEL로 u-boot(work_mode 패치됨) 업로드+실행
        ▼
③ u-boot-sun55iw3p1.bin / u-boot.fex
     헤더의 work_mode 필드가 WORK_MODE_USB_PRODUCT(0x10)이면 →
        │
        ▼
④ U-Boot 부팅 로직이 work_mode 확인 → sunxi_usb_main_loop() 실행
     (cmd/sunxi_sprite.c 로직, 실제로는 board 초기화 경로에서 동일 판단)
        │
        ▼
⑤ EFEX/FES 프로토콜 서버 (drivers/sunxi_usb/usb_efex.c)
     FEX_CMD_fes_down/up/verify_value/query_storage/force_erase 등
        │  macOS 툴이 이 프로토콜로 통신
        ▼
⑥ sys_partition.fex 매핑에 따라 파티션별 이미지 write
     (boot_a, super, vbmeta_a, ... — Android A/B 스킴)
```

> **참고**: fastboot(`drivers/sunxi_usb/usb_fastboot.c`)도 이 SDK에 완전히 구현되어 있지만, `WORK_MODE_BOOT`(정상 부팅) 경로에서만 도달 가능하다. 즉 이미 살아있는 Android 시스템에서 `adb reboot bootloader`로 재진입하는 용도이지, FEL 벽돌 복구에는 쓸 수 없다. (3.3절 참조)

---

## 2. SoC 인식 (FEL 레벨)

`sunxi-tools`(오픈소스, `linux-sunxi/sunxi-tools`)의 `soc_info.c`에 다음이 등록되어 있다.

| SoC | soc_id | sunxi-tools 등록 이름 | 근거 |
|---|---|---|---|
| H616 (T507과 동일 다이) | `0x1823` | `H616` | `soc_info.c:551-556` |
| A523 (T527과 동일 다이) | `0x1890` | `A523` | `soc_info.c:618-630` |

`sunxi-fel`의 지원 커맨드는 순수 FEL(메모리 레벨)뿐이다: `spl / uboot / hex / dump / exe / memmove / readl / writel / read / write / sid / spiflash`. **FES/eMMC 관련 커맨드는 전혀 없다.** 유일한 스토리지 관련 기능(`fel-spiflash.c`)은 SPI NOR 칩에 직접 커맨드를 비트뱅잉하는 것으로, eMMC/FES와 무관하다.

→ **결론**: FEL 레벨(SPL/U-Boot 업로드, 메모리 조작)은 `brew install sunxi-tools`만으로 T507/T527 모두 즉시 사용 가능. 전체 이미지 write(FES)는 별도 구현 필요.

---

## 3. work_mode 메커니즘

### 3.1 정의 (`u-boot-2018/include/spare_head.h:14-27`)

```c
#define WORK_MODE_BOOT           0x00  /* normal boot mode */
#define WORK_MODE_USB_PRODUCT    0x10  /* usb product mode  ← EFEX 진입 */
#define WORK_MODE_CARD_PRODUCT   0x11  /* card burn mode */
#define WORK_MODE_USB_DEBUG      0x12  /* usb efex protocol test mode */
#define WORK_MODE_SPRITE_RECOVERY 0x13 /* 내부 백업 파티션 복구 */
#define WORK_MODE_CARD_UPDATE    0x14  /* sdcard 자동 업데이트 */
#define WORK_MODE_USB_UPDATE     0x20  /* usb update mode */
```

### 3.2 분기 로직 (`u-boot-2018/cmd/sunxi_sprite.c:19-108`)

```c
if (get_boot_work_mode() == WORK_MODE_USB_PRODUCT) {
    printf("run usb efex\n");
    sunxi_usb_dev_register(2);
    sunxi_usb_main_loop(2500);   // ← EFEX 프로토콜 루프 시작
}
```

`WORK_MODE_CARD_PRODUCT`(SD카드 번), `WORK_MODE_SPRITE_RECOVERY`(내부 백업), `WORK_MODE_CARD_UPDATE`/`WORK_MODE_UDISK_UPDATE`(외부 미디어 자동 업데이트) 등은 각각 다른 경로로 분기하며, **fastboot로 가는 분기는 이 함수 안에 존재하지 않는다.**

### 3.3 fastboot가 벽돌 복구에 쓰일 수 없는 이유

fastboot(`drivers/sunxi_usb/usb_fastboot.c`, 2082줄)는 표준 AOSP 와이어 프로토콜(`flash:`/`erase:`/`download:`/`getvar:`/`oem`/`boot`)을 완전히 구현하고 있고, `boot0`/`mbr`/`toc1`(u-boot) 같은 로우레벨 파티션까지 `flash:boot0` 형태로 특수 처리(`__flash_to_boot0`, `__flash_to_mbr`, `__flash_to_uboot`)된다.

**그러나** `board/sunxi/board_common.c`에서 확인한 바로는, fastboot 진입은 `work_mode == WORK_MODE_BOOT`(정상 부팅 경로)에서 U-Boot가 정상적으로 Android 부팅을 시도하다가 특정 리부트 사유(예: `adb reboot bootloader`)를 감지했을 때만 일어난다. **`WORK_MODE_USB_PRODUCT`처럼 FEL로 강제 주입되는 모드에서는 fastboot로 가는 분기 자체가 없다.**

→ eMMC가 완전히 비어있거나 U-Boot 자체가 깨진 진짜 "벽돌" 상태에서는 `WORK_MODE_BOOT` 경로에 도달할 방법이 없으므로, **fastboot는 옵션이 될 수 없다.** EFEX/FES가 유일한 복구 경로다.

### 3.4 work_mode 필드의 정확한 위치

**구조체 정의** (`u-boot-2018/include/private_uboot.h`):

```c
struct spare_boot_ctrl_head {      // 48바이트, offset 0
    unsigned int  jump_instruction;    // +0
    unsigned char magic[8];            // +4   "uboot"
    unsigned int  check_sum;           // +12  ← 패치 후 재계산 필요
    unsigned int  align_size;          // +16  (=0x4000 고정)
    unsigned int  length;              // +20
    unsigned int  uboot_length;        // +24
    unsigned char version[8];          // +28  "4.0.0"
    unsigned char platform[8];         // +36  "2.0.0"
    int           reserved[1];         // +44
};                                  // sizeof = 48

struct spare_boot_data_head {      // offset 48부터
    unsigned int  dram_para[32];       // +0   (128바이트)
    int           run_clock;           // +128
    int           run_core_vol;        // +132
    int           uart_port;           // +136
    normal_gpio_cfg uart_gpio[2];      // +140 (16바이트)
    int           twi_port;            // +156
    normal_gpio_cfg twi_gpio[2];       // +160 (16바이트)
    int           work_mode;           // +176 ★
    ...
};
```

**절대 오프레: `48 (boot_ctrl_head 크기) + 176 (boot_data_head 내 상대 오프셋) = 224 (0xE0)`**

### 3.5 실물 검증

SDK raw 빌드 파일(`u-boot-2018/u-boot-sun55iw3p1.bin`)과 최종 fusing 이미지(`t527_android13_pluto_wallpad_uart0.img`) 양쪽에서 offset 224를 직접 읽어 확인:

| 파일 | offset 224 값 | 의미 |
|---|---|---|
| `u-boot-2018/u-boot-sun55iw3p1.bin` (raw 빌드) | `00 00 00 00` | WORK_MODE_BOOT (패킹 전 빌드 산출물) |
| 최종 이미지 내 `u-boot.fex` (offset 412672) | `00 00 00 00` | WORK_MODE_BOOT (정상 양산 부팅용, 예상대로) |

→ **패치 절차**: 이미지에서 추출한 `u-boot.fex`의 절대 offset 224에 4바이트 `10 00 00 00`을 쓰면 `WORK_MODE_USB_PRODUCT`가 된다.

또한 이 두 파일의 check_sum 필드(offset 12) 비교로 흥미로운 사실 확인:
- raw 빌드본: `check_sum = 0x5f0a6c39` (= `STAMP_VALUE`, 즉 **미계산 placeholder**)
- 최종 이미지 내 `u-boot.fex`: `check_sum = 0xbd9431b1` (**실제 계산된 값**)

→ SDK 빌드 디렉토리의 `.bin` 파일은 패킹 전 중간 산출물이며, 실제 배포되는 것은 최종 이미지 안의 것이다. **work_mode 패치 후 체크섬을 반드시 재계산해야 한다** (4절 참조).

---

## 4. 체크섬 알고리즘

출처: `u-boot-2018/tools/mksunxiboot.c: gen_check_sum()`

```c
int gen_check_sum(struct boot_file_head *head_p)
{
    uint32_t length = le32_to_cpu(head_p->length);   // 4바이트 정렬 필수
    uint32_t *buf = (uint32_t *)head_p;
    uint32_t sum = 0;

    head_p->check_sum = cpu_to_le32(STAMP_VALUE);   // 0x5F0A6C39로 임시 채움
    for (uint32_t i = 0; i < (length >> 2); i++)
        sum += le32_to_cpu(buf[i]);                  // 전체 파일을 4바이트 워드로 합산

    head_p->check_sum = cpu_to_le32(sum);            // 계산된 합을 다시 기록
    return 0;
}
```

암호화 서명이 아닌 **단순 32비트 워드섬**이라 macOS 툴에서 그대로 재구현 가능하다. `head_p->length` 필드(offset 20)가 가리키는 범위 전체를 대상으로 한다.

> **미확인 사항**: FEL로 직접 업로드하는 경우에도 BROM/이전 스테이지가 이 체크섬을 실제로 검증하는지는 소스 추적만으로는 확답할 수 없다. 실기 테스트 필요.

---

## 5. sys_partition.fex (파티션 스펙)

Android A/B(Seamless Update) 스킴을 사용하는 파티션 테이블 스펙 파일. 주요 특징:

- `[mbr] size = 16384` → KB 단위 (16MB)
- 개별 파티션은 `size = 32M` 등 명시적 단위 표기
- A/B 슬롯: `bootloader_a/b`, `env_a/b`, `boot_a/b`, `vendor_boot_a/b`, `init_boot_a/b`, `vbmeta*_a/b`, `dtbo_a/b`
- `super` 파티션(3.5G) — Android dynamic partition, `system`/`vendor`/`product`가 여기 통합됨
- `UDISK` 파티션은 **size 필드 없음** → "남은 공간 전부"를 의미하는 특수 엔트리 (파서가 반드시 처리해야 함)
- `user_type`(0x8000/0x8100), `keydata`(재양산 시 데이터 보존), `ro` 플래그 등의 정확한 비트 의미는 추가 확인 필요

---

## 6. EFEX/FES 프로토콜 커맨드 세트

출처: `u-boot-2018/drivers/sunxi_usb/usb_efex.c` (2467줄)

```c
FEX_CMD_fes_trans           // 전송 준비
FEX_CMD_fes_run             // 코드 실행
FEX_CMD_fes_down            // 쓰기 (다운로드 = 호스트→디바이스)
FEX_CMD_fes_up              // 읽기 (업로드 = 디바이스→호스트)
FEX_CMD_fes_verify_value    // CRC 검증
FEX_CMD_fes_verify_status
FEX_CMD_fes_query_storage   // 스토리지 타입 쿼리 (eMMC/NAND/SPI-NOR)
FEX_CMD_fes_flash_set_on/off
FEX_CMD_fes_flash_size_probe
FEX_CMD_fes_tool_mode
FEX_CMD_fes_memset
FEX_CMD_fes_pmu
FEX_CMD_fes_unseqmem_read/write
FEX_CMD_fes_force_erase
FEX_CMD_fes_force_erase_key
FEX_CMD_fes_query_secure
FEX_CMD_fes_query_info
```

모드 전환 태그: `AL_VERIFY_DEV_TAG_DATA`, `AL_VERIFY_DEV_MODE_SRV` (`usb_efex.c:943-946`) — 원 제안서 4절에서 설명한 FEL→FES 전환 구조가 실물 소스로 확인됨.

> **다음 조사 필요**: 각 `FEX_CMD_*` 커맨드의 정확한 요청/응답 구조체 필드(바이트 레이아웃)는 아직 상세 분석하지 않음. macOS 클라이언트 구현 시 `usb_efex.c`의 `struct global_cmd_s` 및 관련 구조체를 마저 분석해야 함.

---

## 7. IMAGEWTY 컨테이너 포맷 (실물 검증됨)

최종 fusing 이미지 `t527_android13_pluto_wallpad_uart0.img` (1,300,323,328 bytes)를 직접 파싱하여 검증.

### 7.1 메인 헤더 (offset 0x00~0x60)

```
offset 0x00: "IMAGEWTY"           매직
offset 0x0c: 0x00000060 (96)      header_size
offset 0x38: 0x00000400 (1024)    아이템 레코드 크기
offset 0x3c: 0x00000033 (51)      아이템 개수
offset 0x40: 0x00000400 (1024)    (반복 확인용 필드)
```

### 7.2 아이템 레코드 구조 (1024바이트 고정, 여러 파일명의 절대 offset 간격으로 실측 검증)

```
+0x000 (28바이트)  : 해시/체크섬으로 추정되는 값 (미상)
+0x01c ( 4바이트)  : 0x00000100 (256, 고정값)
+0x020 ( 4바이트)  : 0x00000400 (1024, 고정값)
+0x024 ( 8바이트)  : maintype  (예: "BOOT    ", "COMMON  ")
+0x02c (16바이트)  : subtype   (예: "BOOT0_0000000000")
+0x03c ( 4바이트)  : reserved (0)
+0x040~           : filename (null-terminated, "boot0_nand.fex" 등)
+0x140 ( 4바이트)  : stored_length   (예: sys_partition.fex → 5968)
+0x148 ( 4바이트)  : original_length (예: sys_partition.fex → 5964)
+0x150 ( 4바이트)  : file_offset (이미지 내 실제 데이터 절대 offset)
```

### 7.3 실측 검증 결과

| 아이템 | offset(+0x150) | length(+0x148) | 검증 방법 | 결과 |
|---|---|---|---|---|
| `sys_partition.fex` | 101376 | 5964 | 원문 텍스트와 바이트 비교 | **완전 일치** ✅ |
| `boot0_nand.fex` | 269312 | 73728 | 매직 바이트 확인 | `eGON.BT0` 확인 ✅ |
| `u-boot.fex` | 412672 | 950272 | 매직 + work_mode + checksum 확인 | `uboot` 매직, work_mode=0x00, check_sum=0xbd9431b1(계산됨) ✅ |

→ 아이템 테이블 구조가 100% 실물 데이터로 검증되었다. macOS 툴은 이 구조를 파싱해 `sys_partition.fex`/`u-boot.fex`/`boot0_*.fex`/`fes1.fex` 등을 이미지에서 직접 추출할 수 있다.

---

## 8. 실행 절차 요약 (macOS 툴 구현 관점, 11절 실기 로그 반영하여 수정)

> **중요한 구분**: 아래 ①의 "FEL 프라이머"(work_mode 패치됨)와 ⑥의 "저장소에 쓰는 boot0/boot1"(원본, work_mode=0x00)은 **서로 다른 용도의 파일이다.** 패치본을 저장소에 쓰면 안 되고, 원본을 FEL로 올려 실행해도 EFEX 모드에 진입하지 않는다.

```
① IMAGEWTY 이미지 파싱 → sys_partition.fex, fes1.fex, boot0_sdcard.fex(또는 nand),
   boot_package.fex(=BOOT1, u-boot+monitor+scp+dtb TOC1 번들) 추출

② "FEL 프라이머" 준비: u-boot.fex(또는 boot_package.fex 내부 u-boot 아이템, 둘은 바이트
   단위로 동일함이 검증됨)의 절대 offset 224를 10 00 00 00 (WORK_MODE_USB_PRODUCT)로 패치
   후 mksunxiboot.c의 gen_check_sum()으로 offset 12 check_sum 재계산.
   → fes1과 이 패치본이 이어서 실행되도록 묶어 업로드해야 함 (11.3절 미해결 이슈 참조 —
     fes1 단독 업로드만으로는 실행이 이어지지 않고 보드가 멈추는 현상 확인됨)

③ FEL로 ②를 업로드+실행 → DRAM 초기화(~4초) → 곧바로 U-Boot(workmode=16) 부팅
   (실기 로그 기준 DRAM은 이 시점에 512MiB로 제한된 상태로 뜸 — 정상)

④ U-Boot가 SD/MMC 등 스토리지에서 정상 부팅 이미지를 못 찾고 카드 인식도 실패 →
   work_mode=0x10 확인 → sunxi_usb_main_loop() 진입 → USB 1f3a:efe8로 EFEX 서버 대기

⑤ macOS 툴이 EFEX 프로토콜로:
   - SUNXI_EFEX_ERASE_TAG: 전체/파티션 erase
   - MBR/GPT write (sys_partition.fex 파싱 결과 기반, frp/private 등 keydata=1
     파티션은 지우기 전에 먼저 읽어 보존)
   - FEX_CMD_fes_down 등으로 각 파티션(super/boot_a/vbmeta_a/... ) write

⑥ SUNXI_EFEX_BOOT1_TAG로 boot_package.fex(원본, work_mode=0x00) write,
   SUNXI_EFEX_BOOT0_TAG로 boot0_sdcard.fex(원본) write
   → storage_type에 따라 nand/sdcard 버전 선택 (로그에서는 storage type=2=SD/eMMC)

⑦ SUNXI_UPDATE_NEXT_ACTION_REBOOT → 디바이스 재부팅 → 방금 쓴 boot0/boot1로
   정상 부팅(workmode=0, DRAM 2GiB, BL31 → U-Boot → Android) 확인
```

---

## 9. 남은 미확인 사항 (11절 실기 검증 반영하여 갱신)

1. ~~FEL 업로드 시 체크섬이 실제로 검증되는가~~ — 여전히 미확인이지만 우선순위 낮춤.
2. **`usb_efex.c`의 각 `FEX_CMD_*` 요청/응답 구조체 필드 상세 분석** — 클라이언트 구현 전 필수. (11.2절 로그로 일부 태그의 존재/크기는 확인했으나 정확한 바이트 레이아웃은 미분석)
3. ~~toc0/toc1 vs eGON 포맷~~ — **해결됨**: `toc0.fex`/`toc1.fex`는 8바이트짜리 미사용 placeholder였고, 실제로는 `boot0_nand.fex`/`boot0_sdcard.fex`(eGON) + `boot_package.fex`(TOC1, u-boot+monitor+scp+dtb)가 쓰인다 (11.2절).
4. `sys_partition.fex`의 `user_type`(0x8000/0x8100) — **일부 해결**: 실기 로그의 MBR dump에서 일반 파티션은 32768(0x8000), UDISK만 33024(0x8100)로 재확인됨. 정확한 비트 의미(플래그 조합)는 미상.
5. T507(H616 계열) SDK에서도 동일한 구조/오프셋이 적용되는지 별도 확인 (이번 조사는 T527 SDK만 대상으로 함).
6. ~~"BOOTLOADER" GPT 파티션(boot-resource.fex)이 SPL/BOOT0/BOOT1을 포함하는가~~ — **완전히 해결됨(11.5절)**: 파일 내용물(boot-resource.fex, FAT16 폰트/스플래시 리소스뿐)과 pack 스크립트(boot_resource_list/boot_file_list 완전 분리)로는 무관함을 확인했지만, **실제 UART 로그 실측 결과 "bootloader_a만 체크"해도 PhoenixSuit이 28개 파티션 전부 + BOOT1_TAG + BOOT0_TAG를 포함한 전체 재퓨징을 실행**함을 확인했다. 즉 bootloader 파티션 선택이 "전체 재퓨징 트리거"로 동작하는 것이지, 그 파티션 내용물에 boot0/uboot가 들어있는 게 아니다.
7. ~~fes1 단독 업로드 시 보드 응답 불능~~ — **해결됨(11.3절)**: 원인은 `sunxi-fel spl`이 fes1을 잘못된 주소(0x44000, boot0용)에 올렸기 때문. 정확한 주소(0x4C000, `include/configs/sun55iw3p1.h`에서 계산 및 `fes1.lds`로 재확인)로 raw write+exec하니 DRAM 초기화까지 정상 성공(DRAM read/write 테스트로 확인). 남은 것은 u-boot/boot_package를 이어서 올바른 주소에 올려 EFEX 모드 진입까지 확인하는 것.
8. **(신규) EFEX BOOT0_TAG/BOOT1_TAG 명령의 정확한 요청 포맷** — `usb_efex.c`의 `FEX_CMD_*` 목록에는 명시적인 "boot0"/"boot1" 커맨드가 안 보였는데, 로그의 `SUNXI_EFEX_BOOT1_TAG`/`SUNXI_EFEX_BOOT0_TAG` 문자열이 어느 `FEX_CMD_*`를 통해 전달되는지 (아마 `fes_down`의 서브타입이거나 별도 태그 필드) 확인 필요.

### 9.1 pack 스크립트로 확인한 파일 매핑 (참고용)

`device/softwinner/common/vendorsetup.sh`의 `_package()` → `longan/build/pack` 스크립트(`boot_file_list`, 239~257행)에서 확인한 원본 바이너리 → `.fex` 매핑:

| 원본 (longan 빌드 산출물) | 패킹된 이름 (.fex) |
|---|---|
| `boot0_nand_${CHIP}.bin` | `boot0_nand.fex` |
| `boot0_sdcard_${CHIP}.bin` | `boot0_sdcard.fex` |
| `boot0_spinor_${CHIP}.bin` | `boot0_spinor.fex` |
| `fes1_${CHIP}.bin` | `fes1.fex` |
| `fes1_uart_${CHIP}.bin` | `fes1_uart.fex` |
| `u-boot-${CHIP}.bin` | `u-boot.fex` |
| `u-boot-crashdump-${CHIP}.bin` | `u-boot-crash.fex` |
| **`bl31.bin` / `bl31_${BOARD}.bin`** | **`monitor.fex`** |
| `scp.bin` | `scp.fex` |
| `optee_${CHIP}.bin` | `optee.fex` |
| `opensbi_${CHIP}.bin` | `opensbi.fex` |

이 목록은 `boot_resource_list`(bmp/ini/wavefile → `boot-resource.fex`)와 **완전히 분리된 별도 배열**이다 — 즉 pack 단계에서부터 UI 리소스와 부트체인 바이너리가 섞일 여지가 없다.

---

## 11. 실기 검증 (Rust 구현 + Windows PhoenixSuit 실측 로그)

### 11.1 Rust CLI(`aw-tool`) 구현 및 검증 결과

`Cargo.toml` 기반 Rust 프로젝트(`src/imagewty.rs`, `src/sunxi_head.rs`, `src/fel.rs`)로 아래를 구현하고 전부 실물 데이터/실기로 검증했다.

| 기능 | 검증 방법 | 결과 |
|---|---|---|
| IMAGEWTY 파서 (51개 아이템) | 실제 이미지로 `list`/`extract` 실행 | 전체 아이템 offset/length 정상 파싱, `sys_partition.fex` 추출 결과가 원문과 바이트 단위 일치 |
| work_mode 패치 + 체크섬 재계산 | `u-boot.fex`에 패치 적용 | work_mode 0x00→0x10, check_sum이 정확히 +0x10만큼 증가(0xbd9431b1→0xbd9431c1) — 워드섬 알고리즘 특성상 수학적으로 검증됨 |
| FEL USB 프로토콜 (직접 재구현, sunxi-fel 미사용) | 실기(T527 pluto_wallpad 보드)에 `fel-version` 커맨드 실행 | `soc_id=0x1890(A523) protocol=0x0001 scratchpad=0x61500` — `sunxi-fel version`과 완전히 동일한 결과 |

FEL 프로토콜은 `sunxi-tools`의 `fel_lib.c`를 참고해 그대로 재구현했다 (다음 요소들을 rusb로 이식):
- USB 요청 envelope: `signature="AWUC"`(8B) + `length`(4B) + `unknown1=0x0c000000`(4B) + `request`(2B: `0x11`=READ/`0x12`=WRITE) + `length2`(4B) + `pad`(10B) = 32바이트
- 응답: 13바이트, `signature="AWUS"`로 시작
- FEL 요청: `request`(4B: `0x001`=VERSION/`0x101`=WRITE/`0x102`=EXEC/`0x103`=READ) + `address`(4B) + `length`(4B) + `pad`(4B) = 16바이트
- VID:PID = `0x1f3a:0xefe8`, bulk IN/OUT 엔드포인트는 active config descriptor에서 스캔

### 11.2 실기 fusing 로그 분석 (Windows PhoenixSuit, 동일 "pluto_wallpad" 보드)

사용자가 제공한 실제 PhoenixSuit 전체 이미지 다운로드 로그로 아래를 확인:

- `fes begin commit:1cbb5ea8b3` → DRAM 초기화 시작(`[866]`) → 완료(`[4936]`)까지 **약 4초 소요**. 정상 케이스에서도 이 정도 시간이 걸린다.
- DRAM 초기화 완료 직후 **곧바로 U-Boot 배너 출력** — fes1이 리턴한 뒤 U-Boot로 자동 진행됨을 시사 (호스트의 별도 FEL 커맨드 없이 이어지는 것으로 보임, 단 USB 트래픽 자체는 로그에 안 남으므로 100% 확정은 아님).
- 이 시점 U-Boot: `DRAM: 512 MiB`(제한된 창), `workmode = 16,storage type = 0` — **0x10 = WORK_MODE_USB_PRODUCT 정확히 일치**.
- SD/MMC 카드 인식 시도 반복 실패 → `sunxi work mode=0x10` → `run usb efex` (정확히 `cmd/sunxi_sprite.c` 로직대로 동작).
- EFEX 세션에서 `SUNXI_EFEX_ERASE_TAG` → MBR 덤프(28개 파티션, `sys_partition.fex`와 순서/오프셋 일치) → `frp`/`private`(keydata=1) 파티션은 지우기 전에 먼저 읽어 보존 → GPT primary/backup write → 각 파티션 데이터 write.
- **`SUNXI_EFEX_BOOT1_TAG`**: `boot1 size = 0x150000`(1,376,256 bytes) = **`boot_package.fex`와 정확히 같은 크기** → BOOT1 = boot_package.fex(u-boot+BL31+scp+dtb 번들) 확정.
- **`SUNXI_EFEX_BOOT0_TAG`**: `boot0 size = 0x11000`(69,632 bytes) = **`boot0_sdcard.fex`와 정확히 같은 크기**(storage type=2=SD/eMMC 경로라 sdcard 버전 사용) → BOOT0 = boot0_sdcard.fex 확정.
- `SUNXI_UPDATE_NEXT_ACTION_REBOOT` → 재부팅 → `HELLO! BOOT0 is starting!` → `Loading boot-pkg Succeed(index=0)` → `Entry_name = u-boot/monitor/scp/dtb` (11.2절 TOC1 파싱 결과와 정확히 일치) → BL31 시작 로그(`NOTICE: BL31: v2.5(debug)...`) → 두 번째 U-Boot: **`workmode = 0`, `DRAM: 2 GiB`** (정상 부팅 모드, 풀 DRAM).

**핵심 정정**: work_mode=0x10로 패치하는 대상은 오직 "FEL로 최초 업로드해 EFEX 모드에 진입시키는 프라이머"뿐이다. `SUNXI_EFEX_BOOT0_TAG`/`BOOT1_TAG`로 실제 저장소에 쓰는 boot0/boot1은 **원본 그대로(work_mode=0x00)**여야 한다 — 그래야 재부팅 후 정상 부팅(workmode=0)이 된다. 이 둘을 같은 파일로 착각하면 안 된다.

### 11.3 해결됨: fes1 단독 업로드 시 보드 응답 불능 — 원인은 로드 주소 오류

**증상**: `sunxi-fel spl fes1.fex`(패치 없이 fes1만 단독 업로드)를 실기에 시도한 결과, DRAM 초기화 도중/직후로 추정되는 시점에 USB 응답이 완전히 끊겼다 (`usb_bulk_send() ERROR -7: Operation timed out`, 이후 모든 FEL 커맨드 timeout). 250ms→6초로 readback 대기시간을 늘려 재시도해도 동일하게 실패(이번엔 `usb_bulk_send` 자체가 실패 — 단순 timing 문제가 아님을 시사). 전원 재인가로는 매번 정상 복구됨(영구 손상 아님).

**근본 원인 (소스로 확정)**: `sunxi-fel spl <file>` 커맨드는 A523의 SPL 로드 주소를 `soc_info.c`의 `spl_addr = 0x44000`으로 가정한다. 그런데 fes1은 이 주소용이 아니다:

```
include/configs/sun55iw3p1.h:
  CONFIG_SYS_SRAMA2_BASE = 0x40000
  CONFIG_BOOT0_RUN_ADDR  = 0x40000 + 0x4000 = 0x44000   ← boot0/SPL용 (sunxi-tools의 spl_addr과 일치)
  CONFIG_FES1_RUN_ADDR   = 0x44000 + 0x8000 = 0x4C000   ← fes1 전용, 32KB 높은 별도 주소

fes/fes1.lds: ". = 0x4c000"  ← 링커 스크립트로 재확인됨
```

`sunxi-fel spl fes1.fex`는 fes1을 **잘못된 주소(0x44000, boot0용)**에 올려 실행시켰던 것이고, fes1 내부의 절대주소 참조가 전부 어긋나 크래시가 났던 것으로 보인다. `sunxi-fel`의 `spl`/`uboot` 편의 커맨드는 애초에 mainline U-Boot SPL(`u-boot-sunxi-with-spl.bin`) 관례를 겨냥한 것이라 벤더 전용 `fes1`에는 맞지 않는 도구였다.

**검증**: raw 커맨드로 정확한 주소에 직접 write+exec:
```
sunxi-fel write 0x4c000 fes1.fex
sunxi-fel exe   0x4c000
(6초 대기)
sunxi-fel version        →  정상 응답 (크래시 없음)
sunxi-fel writel 0x40000000 0xdeadbeef
sunxi-fel readl  0x40000000            →  0xdeadbeef 그대로 반환 = DRAM 실제 초기화 확인
```
DRAM read/write가 정상 동작하는 것으로 fes1이 올바른 주소에서 실행되어 DRAM 초기화까지 실제로 성공했음을 확인했다. 이후에도 디바이스는 계속 FEL 응답을 유지한다 (u-boot를 아직 잇지 않았으므로 EFEX 모드까지는 아직 진입하지 않은 상태).

**다음 단계**: 이 정확한 주소(0x4C000)를 기반으로, work_mode 패치된 u-boot(또는 `boot_package.fex`)를 이어서 올바른 주소에 write+exec하여 실제 EFEX 모드(work_mode=0x10, `run usb efex`) 진입까지 확인하는 것이 남았다. u-boot/boot_package의 정확한 로드 주소도 같은 방식(`include/configs/sun55iw3p1.h`)으로 먼저 계산해서 확인할 것.

### 11.3.1 완료: 패치된 u-boot 연결까지 성공 — EFEX 모드 진입 확인 (실기 UART 로그 실증, 2026-09-06)

u-boot의 로드 주소도 소스에서 확정했다: `u-boot.lds`("`. = 0x4A000000`")와 `.config`("`CONFIG_SYS_TEXT_BASE=0x4A000000`")가 정확히 일치. ARM32(elf32-littlearm) 코드로, fes1과 마찬가지로 아직 AArch64 전환 전 단계.

**중요한 교훈**: 처음엔 "u-boot를 DRAM에 먼저 write, fes1은 그 다음에 실행"으로 시도했다가 write 자체가 timeout나며 보드가 멈췄다 — **DRAM은 fes1이 실행되어 초기화되기 전까지 접근 자체가 안 된다.** 올바른 순서는 반드시:
```
1. fes1.fex를 0x4C000에 write
2. 0x4C000에서 exec (DRAM 초기화, ~5~6초 소요)
3. DRAM 초기화 완료 대기
4. work_mode=0x10 패치된 u-boot.fex를 0x4A000000(DRAM)에 write
5. 0x4A000000에서 exec
```

이 순서로 실행한 결과, 실기 UART 로그에서 다음을 확인했다:
```
U-Boot 2018.07 (Aug 02 2026 - 09:36:00 +0900) Allwinner Technology
...
workmode = 16,storage type = 0
...
sunxi work mode=0x10
run usb efex
buf queue page size = 65536
...
usb init ok
set address 0x3 ok
```

11.2절에서 분석한 실제 Windows PhoenixSuit 로그와 **완전히 동일한 지점(work_mode=0x10 → EFEX 모드 진입)까지 자체 구현만으로 도달했다.** 이번 테스트는 승인된 범위(EFEX 진입까지, 저장소 write 없음)를 정확히 지켜 저장소에는 아무것도 쓰지 않았고, 디바이스는 EFEX 세션을 연 채 명령 대기 상태로 남아있다.

**검증된 전체 파이프라인**: IMAGEWTY 추출 → work_mode 패치(offset 224) → 체크섬 재계산 → 정확한 로드 주소(fes1=0x4C000, u-boot=0x4A000000, 둘 다 `include/configs/sun55iw3p1.h`/링커스크립트로 소스 검증) → 올바른 순서로 write+exec = **PhoenixSuit 없이도 자체 툴만으로 EFEX 모드 진입 성공.**

**남은 것**: EFEX 프로토콜 클라이언트(erase/GPT write/파티션 write/BOOT1_TAG/BOOT0_TAG) 구현 — 이게 구현되면 실제 파티션 write까지 자체 툴로 완결된다.

---

## 14. EFEX 프로토콜 클라이언트 구현 (2차 세션)

### 14.1 프로토콜 전체 사양 확보 — `usb_efex.h`에서 그대로

`usb_efex.h`(438줄)에 CBW/CSW 전송 계층과 모든 `FEX_CMD_*`/`APP_LAYER_*` 커맨드의 요청/응답 구조체가 전부 정의되어 있었다. 리버스엔지니어링이 아니라 **벤더 원본 헤더를 그대로 읽은 것**이다.

**전송 계층(CBW/CSW) — FEL의 `aw_usb_request`/응답과 바이트 단위로 완전히 동일**:
```c
struct sunxi_efex_cbw_t {   // 32바이트, "AWUC" 매직
    u32 magic; u32 tag; u32 data_transfer_len;
    u16 reserved_1; u8 reserved_2; u8 cmd_len;
    tTransferData cmd_package;  // direction(1)+resv(1)+dataLen(4)+resv2(10)
};
struct sunxi_efex_csw_t {   // 13바이트, "AWUS" 매직
    u32 magic; u32 tag; u32 residue; u8 status;
};
```
FEL의 `signature[8]`="AWUC\0\0\0\0"는 여기서 magic(4)+tag(4)로 갈라진 것이고, FEL의 `unknown1=0x0c000000`은 reserved_1/reserved_2/cmd_len(=0x0c)이 합쳐진 값, FEL의 `request`(u16)는 `cmd_package.direction`(1바이트)+resv(1바이트)와 정확히 같은 자리 — **두 프로토콜이 처음부터 같은 전송 계층을 공유**한다는 게 바이트 단위로 확인됨.

**명령 opcode**: `APP_LAYER_COMMEN_CMD_VERIFY_DEV`(0x0001)/`SWITCH_ROLE`(0x0002)/`IS_READY`(0x0003), `FEX_CMD_fes_trans`(0x0201)/`fes_run`(0x0202)/`fes_down`(0x0206)/`fes_up`(0x0207)/`fes_verify_value`(0x020C)/`fes_verify_status`(0x020D) 등 전부 확보.

**`fes_down`의 `type` 필드가 바로 BOOT1_TAG 등의 정체**: `usb_efex.c`의 dispatch(1110행)를 보면 `fes_trans_t.type`이 `SUNXI_EFEX_DRAM_MASK`(0x7f00)에 걸리면 특수 목적지(MBR=0x7f01/BOOT1=0x7f02/BOOT0=0x7f03/ERASE=0x7f04)로, 아니면 `addr`을 그냥 **절대 섹터 번호**로 취급해 일반 파티션에 write한다. `dram_data_recv_finish()`(1776행)를 보면 MBR_TAG→`sunxi_sprite_download_mbr()`, BOOT1_TAG→`sunxi_sprite_download_uboot()`, BOOT0_TAG→`sunxi_sprite_download_boot0()`로 이어짐 — 11.2절에서 로그로 확인했던 것과 정확히 일치.

**ERASE_TAG의 실체**: 실제로 지우는 게 아니라 `erase_flag` 값을 디바이스트리 "eraseflag" 속성에 저장만 함 — **이게 PhoenixSuit UI의 "Format Fusing vs Overwrite only" 선택을 그대로 반영하는 자리**라는 것도 확인됨(12절 UI 설계와 직결).

### 14.2 실기 검증: `verify_dev` 완전 성공

Rust로 `src/efex.rs` 구현(CBW/CSW 전송 + `verify_dev`/`fes_down`/`verify_value`/`verify_status`). 첫 시도에서 CBW의 `direction` 값을 반대로 써서(`TL_CMD_TRANSMIT`/`RECEIVE`를 표기 그대로 오용) I/O 에러가 났고, FEL 코드(`AW_USB_WRITE=0x12`가 호스트 송신용)와 대조해 정정 후 재시도:

```
tag="AWUSBFEX" platform_id_hw=0x00161000 platform_id_fw=0x00000001 mode=0x02
```

`platform_id_hw`가 헤더의 `FES_PLATFORM_HW_ID`(0x00161000) 상수와, `mode=0x02`가 `AL_VERIFY_DEV_MODE_SRV`와 정확히 일치 — **자체 구현 EFEX 클라이언트가 실기와 완전한 프로토콜 왕복에 성공**했다.

### 14.3 미해결: `fes_down`(DRAM write, 저장소 무관 안전 테스트)이 매번 타임아웃

`fes_down`으로 안전한 DRAM 주소(flash 무관)에 64바이트를 쓰는 테스트가 반복적으로 실패한다 — 첫 단계(16바이트 fes_down 공지, `fes_trans_t` 구조체)는 CSW까지 정상 응답하는 것으로 보이나, 두 번째 단계(실제 64바이트 데이터 전송)의 CSW 응답을 기다리다 타임아웃. 실패할 때마다 디바이스가 이후 모든 명령에 무응답 상태가 되어 전원 재인가가 필요했다(FEL 때와 달리 이번엔 EFEX 세션 전체 재실행 — fes1→u-boot→EFEX 3단계를 다시 밟아야 함).

`SUNXI_EFEX_TRANS_FINISH_TAG`(0x10000)를 `type`에 OR해야 함(안 하면 다중 청크 전송의 "중간 조각"으로 취급되어 지정 주소에 바로 안 쓰임)을 발견해 추가했으나 동일하게 실패.

`sunxi_efex_state_loop()`(2001행)을 끝까지 추적해 다음을 확인했다 — 요청 구조체 파싱, `SUNXI_EFEX_DATA_TYPE_MASK`/`DRAM_MASK`/`TRANS_FINISH_TAG` 비트 체크, `dram_data_recv_finish()` 호출 조건까지 **논리적으로는 전부 요청과 맞아떨어진다.** 정적 분석만으로는 원인을 특정하지 못했다. 다음 시도 때는:
- fes_down 공지만 보내고 데이터 단계는 생략해서, 두 단계 중 정확히 어디가 문제인지 분리 확인
- `_EFEX_USE_BUF_QUEUE_`(무조건 활성화됨, 53행) 관련 큐잉 로직이 두 번째 CBW 처리에 영향을 주는지 확인
- 필요하면 이 시점에서 USB 패킷 캡처(Wireshark)로 실제 바이트 비교

를 권장.

### 14.4 진단 결과: 원인은 `fes_down` 자체 — `fes_trans`(구버전 명령)로 우회 성공

체계적으로 좁혀나간 결과:

1. **`fes_down` announce만(데이터 생략) 단독 테스트도 실패** — 두 단계 중 첫 번째(16바이트 공지) 자체에서부터 CSW가 안 돌아옴. 데이터 단계 문제가 아니라 announce 자체 처리 문제로 확정.
2. **같은 세션(재연결 없이) 안에서 verify_dev→fes_down 순서로 테스트해도 동일하게 실패** — "프로세스 재연결이 디바이스 상태를 리셋시킨다"는 가설 기각.
3. **`query_storage`(FEX_CMD_*, cmd→response 패턴, receive-data 단계 없음)는 완벽하게 성공** (`storage_type=2`, 실기 로그와 일치) — `FEX_CMD_*` 네임스페이스 자체의 문제가 아니라, "명령 수신 후 추가로 데이터를 더 받는" 패턴에서만 문제가 생긴다는 것을 확정.
4. **`fes_trans`(0x0201, 구버전 커맨드, `fes_trans_old_t` 비트필드 구조체)로 완전히 동일한 목적(DRAM에 write)을 시도 — announce, 데이터 전송 두 단계 전부 CSW status=0(성공)으로 완료됨.**

**결론**: `fes_down`(0x0206) 자체(또는 그 특정 처리 경로)에 이 SDK/보드 조합에서 문제가 있고, 같은 "receive more data" 패턴이라도 **`fes_trans`(0x0201)는 정상 동작**한다. macOS 툴은 DRAM/일반 데이터 전송에 `fes_down` 대신 `fes_trans`를 사용하는 것으로 설계 방향을 바꾼다. (단, `fes_trans`는 `type` 필드가 없고 다운로드 시 항상 `SUNXI_EFEX_DRAM_TAG`로 고정되므로 — MBR/BOOT1/BOOT0/파티션처럼 목적지를 구분해야 하는 write는 `fes_down`이 반드시 필요할 가능성이 있음. 이 부분이 실제로 되는지는 아직 미확인 — 다음 확인 필요.)

**남은 이슈(해결됨, 14.5절 참조)**: `fes_trans_down`이 완전히 성공(양쪽 CSW status=0)한 직후에도 디바이스가 다시 응답 불능이 됨. 쓰기 자체는 성공한 것으로 보고됐으므로, 이건 "쓰기 실패"가 아니라 "쓰기 이후 상태" 관련 별개 이슈로 보임.

### 14.5 근본 원인 확정 및 해결: "세션당 명령 1개" 제약의 정체

`sunxi_efex_state_loop()`를 라인 단위로 재추적해서 확정:

**버그**: 명령의 데이터 phase가 끝나면 코드가 `sunxi_usb_efex_app_step = SUNXI_USB_EFEX_APPS_STATUS`로 설정하고 CSW를 보내지만(`case SUNXI_USB_EFEX_STATUS:`), **이 핸들러가 `app_step`을 IDLE로 리셋하지 않는다.** 다음 명령의 CBW는 당연히 `direction=RECEIVE`(새 명령 바이트를 보내는 것이므로)인데, `app_step`이 여전히 `APPS_STATUS`로 남아있는 상태에서 SETUP 단계 디스패처는 `direction==TRANSMIT`만 허용한다(`else if (app_step == SUNXI_USB_EFEX_APPS_STATUS)` 분기, usb_efex.c:2138). RECEIVE가 오면 `"usb transfer direction is transmit only"`를 출력만 하고 **아무 응답도 안 보낸다** — 이게 두 번째 이후 명령이 전부 타임아웃났던 정확한 원인이다.

**해결**: 매 명령이 끝난 뒤, `direction=TRANSMIT`이고 길이 8바이트(`__sunxi_usb_efex_fill_status()`가 채우는 `Status_t` 크기와 일치)인 "플러시" 라운드를 하나 더 보내면, 이 direction 체크를 통과시켜 `app_step`이 즉시 `APPS_IDLE`로 리셋되고 이어서 SEND_DATA+STATUS 사이클이 한 번 더 돌면서 진짜로 다음 명령을 받을 준비가 된다. 구조적으로 기존 `read_payload()`와 동일(CBW+8바이트 수신+CSW)하므로 그대로 재사용 가능:

```rust
fn flush_status(&mut self) -> Result<()> {
    self.read_payload(8)?;
    Ok(())
}
```

`verify_dev()`/`query_storage()`/`fes_trans_down()` 등 모든 "완결된 명령" 끝에 이 호출을 추가.

**실기 검증**: 같은 세션에서 `fes_trans_down`(64바이트 DRAM write) → `verify_dev` → (별도 프로세스)`verify_dev` → (별도 프로세스)`query_storage` **4개 명령이 연속으로 전부 성공**. "세션당 명령 1개"라는 그동안의 모든 실패 패턴을 설명하고 완전히 해결했다 — 이제 하나의 EFEX 세션(부팅 1회) 안에서 여러 명령을 연쇄 실행할 수 있다. 실제 퓨징 플로우(erase→GPT→파티션 N개→BOOT1→BOOT0)가 재부팅 없이 가능해졌다는 뜻.

**참고**: `fes_down`은 이 수정과 별개로 세션의 *첫* 명령으로 시도했을 때도 실패했으므로, 이 app_step 버그와는 무관한 별도 문제가 있는 것으로 보인다 — 아래 14.6절에서 해결.

### 14.6 `fes_down` 자체의 원인 확정 및 해결: `flash_set_on` 사전 호출 필요

`fes_trans`는 목적지 `type`을 항상 `SUNXI_EFEX_DRAM_TAG`로 하드코딩하고 주소를 그대로 메모리 포인터로 쓰기 때문에 **DRAM 주소 write 전용**이다 — MBR/BOOT1/BOOT0/파티션 write처럼 목적지를 구분해야 하는 작업에는 애초에 쓸 수 없다. 이를 지원하는 건 `fes_down`(0x0206)뿐이므로, 이 명령을 반드시 살려야 실제 퓨징이 가능하다.

`FEX_CMD_fes_flash_set_on`(0x020A)의 핸들러를 확인하니 `sunxi_sprite_init(0)`을 호출한다 — flash/MBR 서브시스템 자체의 초기화 함수다. **이걸 먼저 호출하지 않고 `fes_down`을 쓰면 응답이 아예 안 옴**이라는 가설을 세우고 실기로 검증:

```
flash_set_on() → fes_down_announce() 성공  (announce 단계만)
flash_set_on() → fes_down(전체: announce+데이터+flush) → verify_dev(같은 세션)  → 3개 전부 성공
```

**결론**: `fes_down`을 쓰기 전에 반드시 `FEX_CMD_fes_flash_set_on`을 한 번 호출해야 한다(플래시 서브시스템 미초기화 상태에서는 무응답). 이걸로 MBR_TAG/BOOT1_TAG/BOOT0_TAG/파티션 write까지 전부 가능한 상태가 확보됐다.

**실제 퓨징 세션의 올바른 커맨드 순서(확정)**:
```
1. verify_dev()               (선택, 상태 확인용)
2. flash_set_on()             (필수 — sunxi_sprite_init)
3. fes_down(ERASE_TAG, ...)    (erase_flag 설정)
4. fes_down(MBR_TAG, sunxi_mbr.fex)   (GPT write)
5. fes_down(섹터오프셋, 각 파티션 파일)  × N   (실제 파티션 데이터)
6. fes_down(BOOT1_TAG, boot_package.fex)  (원본, work_mode=0x00)
7. fes_down(BOOT0_TAG, boot0_sdcard.fex)  (원본)
8. flash_set_off()             (sunxi_sprite_exit)
```
매 `fes_down`/`flash_set_on`/`flash_set_off` 호출 뒤에는 `flush_status()`를 반드시 호출해야 다음 커맨드가 처리된다(14.5절).

### 14.7 큰 파티션은 청크 분할 필수 — 수신 버퍼 크기 제한

`misc` 파티션(16MB)을 통째로 한 번에 `fes_down`으로 보냈다가 `bulk_send` 타임아웃 + 디바이스 무응답(재부팅 필요) 발생. 원인은 헤더의 버퍼 크기 제한:

```c
#define SUNXI_EFEX_RECV_MEM_SIZE (4 * 1024 * 1024)   // 4MB (SPINOR면 2MB)
```
그리고 flash 경로는 이 버퍼의 절반만 사용(`base_recv_buffer + SUNXI_EFEX_RECV_MEM_SIZE/2`, usb_efex.c:1133) — 즉 **한 번의 `fes_down` 데이터 phase는 2MB를 절대 넘으면 안 된다.**

flash 경로는 DRAM 경로와 달리 FINISH_TAG 개념이 없다 — 데이터가 도착하는 즉시 `sunxi_flash_write()`가 무조건 실행되므로, **각 `fes_down` 호출 자체가 독립적으로 완결된 하나의 write**다. 따라서 큰 파티션은 여러 개의 완전한 `fes_down` 사이클(각각 announce+data+flush)로 나누고, 섹터 오프셋을 청크 크기만큼씩 전진시켜야 한다.

`write_partition()`을 1MB 청크 단위로 분할하도록 수정했으나 **실기 테스트 결과 1MB도 여전히 실패**(bulk_send 타임아웃, 재부팅 필요). 크기를 점진적으로 낮춰가며 실기로 직접 한계를 찾음:

| 크기 | 결과 |
|---|---|
| 16MB (전체) | 실패 |
| 1MB | 실패 |
| 256KB | 실패 |
| **64KB** | **성공** |
| 4KB | 성공 |

**실제 안전 한계는 헤더에서 계산한 2MB보다 훨씬 낮은 64KB~256KB 사이**였다 (정확한 실측 성공 최대치는 64KB, 256KB는 실패). 이론적 버퍼 한도가 아니라 다른 제약(실제 eMMC write 지연시간이 10초 타임아웃을 넘기거나, USB/DMA 청크 한계 등)이 실질적 병목으로 보인다. `FLASH_CHUNK_BYTES=64KB`로 수정. MBR(64KB)/BOOT1(1.3MB)/BOOT0(64KB)은 이 한계보다 크거나 비슷하므로 이들도 청크 분할이 필요할 수 있음 — 별도 확인 필요.

### 14.8 실기 검증: `misc` 파티션(16MB) 전체 write 완전 성공 — 자체 툴로 첫 실제 파티션 write

`FLASH_CHUNK_BYTES=64KB`로 수정한 뒤, `flash-partition` 커맨드로 실제 이미지의 `misc.fex`(16,777,216 bytes)를 `misc` 파티션(섹터 0x790400, sys_partition.fex에서 계산)에 전체 write:

```
$ aw-tool flash-partition <image> <sys_partition.fex> misc
wrote partition 'misc' (16777216 bytes from misc.fex) at sector 0x790400
```

**256개의 64KB 청크(각각 독립된 announce+data+flush 사이클) 전부 성공**, write 완료 후에도 디바이스가 정상 응답(`efex-verify-dev` 성공) — 세션이 끊기지 않고 이어서 다음 파티션을 계속 쓸 수 있는 상태.

이걸로 **자체 macOS 툴(Rust)만으로 IMAGEWTY 추출 → sys_partition.fex 파싱 → 섹터 오프셋 계산 → EFEX 프로토콜로 실제 eMMC/SD에 파티션 전체 write**까지 end-to-end 파이프라인이 실기로 검증됐다. PhoenixSuit 없이 실제 퓨징의 핵심 동작이 재현 가능함을 확인.

### 14.9 MBR/BOOT0/BOOT1 write 전부 성공 — 파편화된 청크 전송 방식 확정

MBR/BOOT1/BOOT0(전부 `SUNXI_EFEX_DRAM_MASK` 범위 태그)는 flash 경로와 달리 **여러 청크로 나눠 보내고 마지막 청크에만 `TRANS_FINISH_TAG`를 붙이는 방식**이 원래 지원된다는 걸 소스로 확인했다: `fes_down`의 dispatch(usb_efex.c:1110-1144)는 `type`이 정확히 `DRAM_MASK|FINISH_TAG`일 때만 수신 포인터를 `base_recv_buffer`로 리셋하는 특수 케이스이고, 그 외 DRAM_MASK 태그(MBR/BOOT1/BOOT0/ERASE)는 전부 `base_recv_buffer + to_be_recved_size`에 누적하는 일반 분기를 타므로, 같은 `base_type`으로 여러 번 호출해도 누적되다가 FINISH_TAG가 붙은 마지막 청크에서만 `dram_data_recv_finish()`가 실제로 실행된다(`sunxi_sprite_download_{mbr,uboot,boot0}()` 호출). 단, 매 청크가 끝날 때마다 `app_step`이 `APPS_STATUS`로 남는 건 flash 경로와 동일하므로, **청크마다 개별적으로 `flush_status()`가 필요**하다.

`fes_down_chunked(base_type, data)`로 일반화 구현(청크 크기는 flash 경로와 동일한 64KB 사용, 마지막 청크에만 FINISH_TAG OR).

**실기 검증 — 한 세션에서 연쇄로 전부 성공**:
```
$ aw-tool flash-mbr <image>
wrote MBR (65536 bytes) from sunxi_mbr.fex
$ aw-tool flash-boot0 <image> --item boot0_sdcard.fex
wrote BOOT0 (69632 bytes) from boot0_sdcard.fex        (2개 청크: 65536+4096)
$ aw-tool flash-boot1 <image>
wrote BOOT1 (1376256 bytes) from boot_package.fex      (~21개 청크)
```
각 write 직후 `efex-verify-dev`로 디바이스 응답 확인 — 매번 정상. **MBR/BOOT0/BOOT1/일반 파티션(misc, 14.8절) 전부 자체 툴로 write 성공**, 하나의 EFEX 세션에서 여러 write를 연쇄 실행 가능함을 재확인.

**남은 것**:
- `super`(3.5G)처럼 훨씬 큰 파티션에서 수천 개 청크를 보낼 때도 안정적인지 확인 필요 (misc는 256개 청크로 검증됨, super는 ~56000개 청크 규모라 시간이 오래 걸릴 수 있음)
- 전체 시퀀스(erase_flag → MBR → 전체 파티션 N개 → BOOT1 → BOOT0 → 재부팅) 오케스트레이션 및 실기 전체 통합 테스트 — 재부팅 후 실제로 정상 부팅되는지 최종 확인
- work_mode 패치를 안 한 원본 BOOT0/BOOT1을 썼는지 재확인 (실수로 패치본을 쓰면 재부팅 후 다시 EFEX로 빠짐 — 이번 테스트에서는 이미지에서 직접 추출한 원본을 사용해 올바르게 처리됨)

### 11.4 `boot-resource.fex`(GPT "bootloader_a/b" 파티션)의 실제 내용물 — SPL/BOOT0/BOOT1 아님

사용자가 "BOOTLOADER_A를 재기록했더니 부팅 안 되던 단말이 복구됐다"고 보고하여, `boot-resource.fex`가 pack 단계에서 SPL/BOOT0/BOOT1을 포함하도록 만들어지는지 두 가지 방법으로 직접 검증했다.

**(1) 파일 자체를 FAT16으로 직접 파싱** — `boot-resource.fex`는 진짜 FAT16 파일시스템 이미지였고(부트섹터에 `FAT16   ` 시그니처 존재), 루트 디렉터리에는:
```
FONT24.SFT, FONT32.SFT           폰트
BOOTLOGO.BMP, BOOTLOGO_1.BMP     부팅 로고
FASTBOOT.BMP                     fastboot 로고
YELLOW/ORANGE/RED_WARNING.BMP    충전 경고 스플래시
BAT/ (디렉터리), MAGIC.BIN(512B) 배터리 관련 리소스
```
만 있었다. 실행 바이너리는 전혀 없다.

**(2) pack 스크립트 소스 확인** — `device/softwinner/common/vendorsetup.sh`의 `_package()`가 호출하는 `longan/build/pack`에는 `boot_resource_list`(bmp/ini/wavefile만, → `boot-resource.fex`)와 `boot_file_list`(boot0/fes1/u-boot/bl31.bin→`monitor.fex`/scp/optee, → 각각 별도 `.fex`)가 **소스 코드 수준에서부터 완전히 분리된 배열**로 정의되어 있다 (9.1절 표 참조). 애초에 pack 단계에서 섞일 방법이 없다.

**결론**: `boot-resource.fex`(및 GPT의 `bootloader_a`/`bootloader_b` 파티션)는 부팅 스플래시/폰트/배터리 경고 리소스 전용이며 SPL/BOOT0/BOOT1과 무관하다. "재기록으로 부팅이 복구됐다"는 현장 관찰은 이 파일 내용 자체보다는, 그 write 동작에 딸려오는 GPT/MBR 재기록의 부수효과이거나 PhoenixSuit이 파티션 선택과 무관하게 BOOT0/BOOT1을 매번 재기록하기 때문일 가능성이 유력하다 — **아래 11.5절에서 실측으로 확정됨.**

### 11.5 결정적 확인: "bootloader_a만 체크"해도 전체 재퓨징이 실행된다 (UART 로그 실측, 2026-09-06)

사용자가 실제로 PhoenixSuit에서 **`bootloader_a`만 체크**하고 Upgrade를 실행한 뒤 UART 콘솔 로그(`/Users/jejezz/Downloads/usb_to_com_logs/cu.usbserial-56710052361_20260906_141107.log`)를 확보했다. 결과는 11.2절의 "전체 이미지 다운로드" 로그와 **시퀀스가 완전히 동일**했다:

```
SUNXI_EFEX_ERASE_TAG → MBR DUMP(28개 파티션) →
begin to store data: part 0 name bootloader_a ... part 27 name userdata  (28개 전부)
→ SUNXI_EFEX_BOOT1_TAG (boot1 size=0x150000) → SUNXI_EFEX_BOOT0_TAG (boot0 size=0x11000)
→ SUNXI_UPDATE_NEXT_ACTION_REBOOT → 정상 재부팅(workmode=0, DRAM 2GiB)
```

**즉 "bootloader_a"만 선택해도 28개 파티션 전부 + BOOT1(u-boot 포함) + BOOT0이 전부 재기록된다.** 화면 캡처(12절)가 암시했던 "선택한 파티션만 overwrite"라는 동작은 최소한 `bootloader_a`/`bootloader_b`에는 적용되지 않는 것으로 보인다 — 이 파티션(들) 중 하나라도 선택되면 PhoenixSuit이 (부분 write가 아니라) **전체 재퓨징 플로우 전체를 실행**하는 것으로 추정된다.

**결론 (9절 항목 6 최종 해소)**: "bootloader_a를 업데이트하면 u-boot가 바뀐다"는 사용자의 수년간 경험은 정확했다. 다만 그 이유는 `boot-resource.fex`(bootloader_a의 실제 GPT 파티션 내용물)에 u-boot가 들어있어서가 아니라, **`bootloader_a` 선택이라는 행위 자체가 PhoenixSuit PC 프로그램 안에서 "전체 재퓨징(모든 파티션 + BOOT1/BOOT0)"을 트리거하는 스위치로 동작하기 때문**이다. macOS 툴 설계 시, "bootloader" 계열 파티션이 선택되면 부분 write가 아니라 전체 write(파티션 전체 + BOOT0/BOOT1) 플로우로 자동 전환하는 로직을 반드시 반영해야 한다.

---

## 15. super.fex는 Android sparse 이미지 — 전체 플래싱 성공 (3차 세션, 최종 해결)

### 15.1 증상

flash-all이 프로토콜 레벨에서 완전히 성공(MBR + 12개 파티션 + BOOT1 + BOOT0 + reboot, 에러 0건)하는데도, 재부팅 후 커널은 정상 부팅하다가 **first-stage init에서 마운트를 한 번도 시도하지 않고** 1.3초만에 재부팅되는 무한 루프:

```
[    0.104013][    T1] AW BSP version: ,
[    1.376086][    T1] reboot: Restarting system with command 'bootloader'
```

정상 부팅 로그와 비교하면 이 지점에서 나와야 할 `EXT4-fs (mmcblk0p21)`, `erofs: (device dm-0)`, `init: init second stage started!`가 **전부 없다**. 커널 패닉이 아니라 드라이버 `.shutdown` 훅이 순서대로 도는 정상 `device_shutdown()` 시퀀스 → userspace(first-stage init)가 **의도적으로** `reboot(RESTART2, "bootloader")`를 호출한 것.

### 15.2 원인 좁히기 (배제된 가설들)

동일 증상이 wallpad/lobby **두 이미지 모두**에서 재현되고, 같은 이미지를 **Windows PhoenixSuit로 쓰면 정상 부팅** → 이미지 문제가 아니라 우리 쓰기 파이프라인 문제로 확정. 이후 하나씩 배제:

| 가설 | 검증 방법 | 결과 |
|---|---|---|
| GPT/파티션 테이블 오류 | U-Boot `gpt verify mmc 2`, `part list mmc 2` | ✗ "Verify GPT: success!", 28개 파티션 전부 sys_partition.fex와 일치 |
| `bootreason=usb`가 원인 | `board/sunxi/power_manage.c` 확인 | ✗ PMU 전원 인가 원인(button/irq/usb/charger) 표시일 뿐, EFEX와 무관 |
| `next_mode` 값 오류(1 vs 2) | `usb_efex.c:1386-1435` | ✗ `WORK_MODE_USB_TOOL_PRODUCT` 분기는 nonzero면 무조건 `REBOOT`(=2)로 강제 |
| `flash_set_off()` 누락 | 추가 후 재테스트 | ✗ 증상 동일 (다만 세션 종료 절차로서 올바르므로 유지) |
| erase가 불완전 | flash-all **실행 중** UART 캡처 | ✗ real PhoenixSuit와 100% 동일한 per-partition `mmc erase`(CMD38) 수행 |
| vbmeta 체인 손상 | U-Boot `mmc read`로 실기 덤프 | ✗ vbmeta_a/vbmeta_system_a/vbmeta_vendor_a 전부 로컬 파일과 바이트 일치 |
| super 청크 경계 손상 | 6개 지점(1MB~970MB) 실기 덤프 | ✗ "일치"했으나 **비교 대상이 틀렸음**(아래 참조) |
| super 자체가 원인 | `--skip-larger-than`으로 super만 제외 | ✓ 증상 동일 → 하지만 Windows로 **super만** 다시 쓰면 정상 부팅 → **원인은 super 확정** |

`verify_value`/`verify_status`(FEX_CMD 0x020C/0x020D)로 전수 검증도 시도했으나 우리 구현이 `app_step` 상태를 깨뜨려(`SUNXI_USB_EFEX_APPS_STATUS: INVALID direction`) 폐기함. `sunxi_sprite_part_rawdata_verify()`는 순수 read+`add_sum` 체크섬이라 부수효과가 없어 이 명령 자체는 원인과 무관.

### 15.3 근본 원인

**`super.fex`는 raw 파티션 이미지가 아니라 Android sparse 이미지 컨테이너다.**

```
super.fex 헤더:  3a ff 26 ed ...   → 0xED26FF3A = SPARSE_HEADER_MAGIC
  blk_sz       = 4096
  total_blks   = 917504  → 확장 시 3,758,096,384 B (3.5 GB) = super 파티션 전체(0x700000 섹터)
  total_chunks = 41
  파일 크기     = 1,021,182,504 B (973 MB, 압축된 상태)
```

정상 동작(PhoenixSuit) 디바이스와 우리 쓰기 결과를 실기에서 비교하면 원인이 한눈에 드러난다:

| 파티션 내 오프셋 | 정상 디바이스 | 우리가 쓴 결과 |
|---|---|---|
| 0 | `00 00 00 00 ...` (liblp 예약 영역) | `3a ff 26 ed ...` ← **sparse 헤더** |
| 4096 | `67 44 6c 61` = `"gDla"` = 0x616c4467 **LP metadata geometry 매직** | `00 00 00 00 ...` |

즉 real PhoenixSuit는 sparse를 **풀어서(unsparse)** 기록하지만, 우리는 컨테이너를 **그대로** 기록했다. 그 결과 파티션 선두에 liblp 메타데이터 대신 sparse 헤더가 놓이고, first-stage init이 dynamic partition(super) 메타데이터를 찾지 못해 **마운트를 시도하기도 전에** bootloader로 재부팅한 것 — 관찰된 증상과 정확히 일치.

이 사실은 그동안의 모든 모순도 설명한다:
- **6개 지점 샘플링이 전부 "일치"했던 이유**: 우리가 쓴 바이트를 sparse 파일 자신과 비교했기 때문. 애초에 비교 대상이 틀렸다.
- **real PhoenixSuit가 super만 `verify_value`를 건너뛴 이유**: raw용(`sunxi_sprite_part_rawdata_verify`)이 아니라 sparse 전용 검증(`sunxi_sprite_part_sparsedata_verify` / `unsparse_checksum`)을 쓰기 때문.
- **super.fex 크기가 512의 배수가 아니었던 이유**(마지막 청크 552B): 파티션 이미지가 아니라 컨테이너 포맷이라서.

벤더 u-boot에 이미 sparse 지원이 들어있다: `sprite/sparse/`, `include/sprite.h`의 `unsparse_checksum()`, `sprite_verify.c:114`의 `sunxi_sprite_part_sparsedata_verify()`.

### 15.4 수정

`src/sparse.rs` 신규 — sparse 헤더(28B) + 청크 헤더(12B) 파싱, 써야 할 영역만 `Segment::Raw`/`Segment::Fill`로 산출. `DONT_CARE`는 건너뛴다(erase_flag=1이 이미 지운 영역이며 벤더 툴 결과도 동일).

`src/efex.rs`에 `write_partition_sparse()` 추가 — 각 세그먼트를 **확장 후 오프셋**에 해당하는 절대 섹터에 기록. `src/main.rs`의 `write_partition_auto()`가 매직으로 sparse를 자동 감지해 raw/sparse 경로를 선택한다.

lobby 이미지 super.fex의 청크 구성:

| 종류 | 블록 | 크기 | 처리 |
|---|---|---|---|
| RAW | 249,312 | 973.9 MB | 각자의 확장 오프셋에 기록 |
| FILL | 928 | 3.6 MB | 패턴 전개 후 기록 |
| DONT_CARE | 667,264 | 2606.5 MB | 스킵(erase 완료 영역) |

전송량은 이전과 거의 같은 977 MB — 달라진 것은 **어디에 쓰느냐**뿐이다.

### 15.5 결과

```
[4.6/12] partition 'super' OK (1021182504 bytes, sparse -> 3584 MB expanded, 977 MB written)
```

**PhoenixSuit 없이 macOS 툴 단독으로 T527 전체 플래싱 → 정상 Android 부팅 성공.** 하드웨어에 쓰기 전에, unsparse 로직이 만들어내는 바이트가 정상 동작 디바이스의 실기 덤프와 모든 확인 지점(0/4096/64K/128K/256K/384K)에서 일치함을 오프라인으로 먼저 검증했다.

**교훈**: `.fex` 아이템은 파일명이 파티션 이름과 같아도 **raw 파티션 이미지라는 보장이 없다**. 새 파티션을 지원할 때는 반드시 매직 바이트로 컨테이너 여부를 먼저 확인할 것.

### 15.6 두 제품 이미지 교차 검증

수정이 특정 이미지에만 맞춘 것이 아님을 확인하기 위해, 처음 문제가 발견된 wallpad 이미지로도 전체 플래싱하여 정상 부팅을 확인했다.

| 이미지 | super.fex | 소요 시간 | super 전송량 | 결과 |
|---|---|---|---|---|
| lobby | 1,021,182,504 B (41 청크) | 1분 42초 | 977 MB (3584 MB 확장) | 정상 부팅 |
| wallpad | 922,378,780 B (40 청크) | 1분 41초 | 882 MB (3584 MB 확장) | 정상 부팅 |

두 이미지 모두 `super.fex`가 sparse이고 확장 크기는 동일하게 3.5 GB(= 파티션 span 0x700000 섹터)다. **두 이미지의 `sys_partition.fex`는 바이트 단위로 완전히 동일**하다(레이아웃 차이 없음).

### 15.7 주소 공간 두 가지: addrlo vs GPT LBA (주의)

파티션 오프셋은 **서로 다른 두 공간**으로 표기되며, 항상 정확히 `0xa000` 섹터(20 MB) 차이가 난다. 같은 PhoenixSuit 실행 로그 안에 둘 다 찍히므로 혼동하기 쉽다:

| 파티션 | MBR DUMP (`addrlo`) | GPT 덤프 (LBA) | 차이 |
|---|---|---|---|
| bootloader_a | 0x8000 | 0x12000 | 0xa000 |
| super | 0x90400 | 0x9a400 | 0xa000 |
| misc | 0x790400 | 0x79a400 | 0xa000 |
| vbmeta_a | 0x798400 | 0x7a2400 | 0xa000 |
| userdata | 0x80a400 | 0x814400 | 0xa000 |

- `sys_partition.fex`에서 계산한 오프셋과 `fes_down`의 섹터 인자는 **addrlo 공간**이다. 디바이스 쪽에서 실제 기록 시 20 MB 오프셋을 더한다.
- U-Boot 셸의 `mmc read`/`part list`로 실기를 덤프할 때는 **GPT LBA 공간**을 써야 한다.

실증: sys_partition 기준 `0x90400`에 쓴 super의 liblp geometry 매직이 GPT LBA `0x9a408`(= `0x90400 + 0xa000` + 8섹터)에서 발견됐다.

> 이 절은 초기에 잘못 기록했던 내용을 정정한 것이다. 한쪽 이미지의 addrlo 값과 다른 쪽 이미지의 GPT LBA 값을 비교해 "두 제품의 파티션 레이아웃이 다르다"고 적었으나, 실제로는 동일한 레이아웃을 두 공간으로 본 것이었다. 같은 이유로 "MBR DUMP의 super=0x90400은 이전 플래싱 잔재"라고 적었던 것도 틀렸다 — `0x90400`은 두 이미지 모두에서 항상 나오는 addrlo 값이다.

---

## 16. FEL 부트스트랩 내장 — 단일 명령 완결 (3차 세션)

15절까지의 플래싱은 성공했지만, EFEX 진입까지는 매번 외부 `sunxi-fel` 바이너리를 손으로 호출해야 했다(fes1 추출 → work_mode 패치 → write/exe → 대기 → u-boot 동일 반복). `src/fel.rs`에는 `write_memory`/`execute`가 이미 구현돼 있었는데 한 번도 쓰이지 않고 있었다.

`src/bootstrap.rs` 신규 — 두 프라이머를 **IMAGEWTY 이미지에서 직접** 꺼내 work_mode 패치(offset 224) + 체크섬 재계산 후 업로드/실행한다:

| 단계 | 대상 | 주소 | 근거 |
|---|---|---|---|
| 1 | `fes1.fex` (work_mode=0x10) | `0x4C000` | `include/configs/sun55iw3p1.h`의 `CONFIG_FES1_RUN_ADDR = 0x44000 + 0x8000`, `fes/fes1.lds`의 `. = 0x4c000` |
| 2 | `u-boot.fex` (work_mode=0x10) | `0x4A000000` | `u-boot.lds`, `.config`의 `CONFIG_SYS_TEXT_BASE` |

**모드 판별**: FEL과 EFEX는 VID:PID(1f3a:efe8)도 CBW 봉투도 같아서 장치 종류만으론 구분이 안 된다. 응답의 mode 필드로 구분한다 — FEL은 버전 요청에 `0x01`, EFEX의 `verify_dev`는 `0x02`(`MODE_SRV`). FEL 장치에 이 프로브를 보내도 버전을 읽는 것뿐이라 무해하므로, 이걸 그대로 "이미 EFEX인가" 판정에 쓴다.

고정 sleep 대신 폴링으로 대기한다(fes1 실행 후 FEL 재등장 최대 30초, u-boot 실행 후 EFEX 등장 최대 30초) — 수동 절차에서 5초 sleep이 가끔 모자랐던 문제가 사라진다.

`flash-all`은 시작 시 자동으로 부트스트랩하며(이미 EFEX면 건너뜀), 부트스트랩은 `flash_set_on`과 모든 쓰기보다 **먼저** 실행되므로 실패해도 장치에 아무것도 쓰이지 않는다. 끄려면 `--no-bootstrap`, EFEX 진입만 하려면 `aw-tool bootstrap <image>`.

**결과**: FEL 상태에서 명령 하나로 부트스트랩 + 전체 플래싱 + 재부팅까지 **1분 42초**에 완주, 정상 Android 부팅 확인. 외부 sunxi-tools 의존 제거.

```
$ aw-tool flash-all <image.img> <sys_partition.fex> --reboot
bootstrap: FEL device found (soc_id=0x1890)
bootstrap: fes1 running at 0x4c000 (47264 bytes), initialising DRAM...
bootstrap: u-boot running at 0x4a000000 (950272 bytes), waiting for EFEX...
bootstrap: EFEX mode reached
[1] flash_set_on OK
...
[4.6/12] partition 'super' OK (1021182504 bytes, sparse -> 3584 MB expanded, 977 MB written)
...
[8] reboot triggered
```

> T507로 옮길 때: 두 로드 주소는 SoC별 값이므로 해당 SDK의 `include/configs/<soc>.h`와 링커 스크립트에서 다시 확인해야 한다. CLI에 `--fes1-addr`/`--uboot-addr`로 노출해 두었다.

---

## 12. UI/UX 설계 참고 (PhoenixSuit 화면 캡처 기반)

사용자가 공유한 PhoenixSuit Windows 화면 캡처(`phoenix_suit_windows.png`)에서 확인한 Firmware 업로드 화면 동작:

1. **파티션을 아무것도 선택하지 않은 상태**로 "Upgrade" 실행 → 사용자에게 **"Format Fusing" 또는 "Overwrite only"** 중 선택하도록 질문한다 (전체 재포맷 후 굽기 vs 기존 데이터 보존하며 덮어쓰기).
2. **특정 파티션(들)을 체크박스로 선택**한 상태로 "Upgrade" 실행 → 질문 없이 **선택된 파티션만 바로 overwrite**한다.

체크박스 목록은 `sys_partition.fex`의 파티션 이름을 그대로 나열한 것으로 보인다(`BOOTLOADER_A`, `ENV_A`, `BOOT_A`, `VENDOR_BOOT_A`, `INIT_BOOT_A`, `SUPER` 등). macOS 툴의 플래싱 모드 설계 시 이 두 갈래(전체 굽기 시 포맷 여부 확인 / 부분 파티션 선택 시 확인 없이 즉시 진행) 그대로 따라가는 것을 권장.

> **주의(11.5절 실측으로 확인)**: "선택한 파티션만 overwrite"라는 동작은 최소한 `BOOTLOADER_A`/`BOOTLOADER_B`에는 적용되지 않는다 — 이 체크박스가 선택되면 실제로는 전체 파티션(28개) + BOOT1(u-boot 포함)/BOOT0가 통째로 재기록된다. macOS 툴 UI 구현 시, "bootloader" 계열 파티션이 선택되면 (부분 write처럼 보이더라도) 내부적으로 전체 재퓨징 플로우로 자동 전환해야 실제 장치 동작과 일치한다. 다른 파티션(super/boot_a 등)만 선택했을 때도 동일하게 전체 재퓨징이 일어나는지는 별도 확인 필요.

---

## 13. 참고 경로 (조사에 사용한 실제 파일)

```
U-Boot 소스:      /Volumes/jejezz/Work/AllWinnerT527/android13/longan/brandy/brandy-2.0/u-boot-2018/
  - include/spare_head.h        (work_mode 상수 정의)
  - include/private_uboot.h     (spare_boot_ctrl_head / spare_boot_data_head 구조체)
  - include/private_toc.h       (TOC0/TOC1 시큐어부트 포맷)
  - cmd/sunxi_sprite.c           (work_mode 분기 로직)
  - drivers/sunxi_usb/usb_efex.c        (EFEX/FES 프로토콜)
  - drivers/sunxi_usb/usb_fastboot.c    (fastboot 프로토콜)
  - tools/mksunxiboot.c          (체크섬 알고리즘)
  - u-boot-sun55iw3p1.bin        (raw 빌드 산출물, 체크섬 미계산 상태)

SPL/fes1 소스:    /Volumes/jejezz/Work/AllWinnerT527/android13/longan/brandy/brandy-2.0/spl-pub/
  - fes/main/fes1_main.c

최종 fusing 이미지: /Volumes/jejezz/Work/AllWinnerT527/android13/longan/out/t527_android13_pluto_wallpad_uart0.img
  (IMAGEWTY 컨테이너, 1,300,323,328 bytes, 51개 아이템)

sunxi-tools (오픈소스): https://github.com/linux-sunxi/sunxi-tools
  - soc_info.c (H616/A523 SoC ID 등록 확인)
  - fel.c (FEL 전용 커맨드셋, FES 없음)
  - fel_lib.c (FEL USB 와이어 프로토콜, aw-tool의 src/fel.rs가 이를 참고해 재구현)

OpenixSuit (참고용, 소스 비공개): https://github.com/YuzukiTsuru/OpenixSuit
  - README의 "Source Code" 섹션 = 실제 코드 아니고 앱 생성용 Claude 프롬프트 원문

macOS 툴 (이 저장소, Rust CLI):
  - src/imagewty.rs      (IMAGEWTY 파서)
  - src/sunxi_head.rs    (work_mode 패치 + 체크섬)
  - src/fel.rs           (FEL USB 프로토콜, rusb 기반)
  - src/efex.rs          (EFEX USB 프로토콜 — 전체 플래싱 검증 완료)
  - src/bootstrap.rs     (FEL→EFEX 자동 진입, fes1/u-boot 프라이머 스테이징, 16절)
  - src/sparse.rs        (Android sparse 이미지 파서 — super.fex unsparse, 15절)
  - src/sys_partition.rs (sys_partition.fex 파서 + 파티션 오프셋 계산)
  - src/main.rs          (CLI: list / extract / patch-workmode / fel-version / efex-* / flash-* / flash-all)

EFEX 프로토콜 사양 (14절 근거):
  - drivers/sunxi_usb/usb_efex.h (전송계층 CBW/CSW, 모든 명령 구조체, TAG 상수)
  - drivers/sunxi_usb/usb_efex.c (sunxi_efex_state_loop 2001행, fes_down 1110행,
    dram_data_recv_finish 1776행)

boot_package.fex 관련 (TOC1 포맷, 이번 조사에서 새로 파싱):
  - head: sbrom_toc1_head_info_t (64B) — magic="sunxi-package", end sentinel=0x3b45494d("MIE;")
  - item: sbrom_toc1_item_info_t (368B) — name[64]+data_offset+data_len+encrypt+type+run_addr+index+...
  - 4개 아이템: u-boot(offset 0x800/950272B), monitor=BL31(offset 0xe8800/70577B),
    scp(offset 0xf9c00/180228B), dtb(offset 0x126000/161792B, 매직 0xd00dfeed 확인)

실기 fusing 로그 (Windows PhoenixSuit, 사용자 제공, 2026-09-06):
  - "pluto_wallpad" 보드 전체 이미지 다운로드 전체 로그, 11절 분석 근거
  - /Users/jejezz/Downloads/usb_to_com_logs/cu.usbserial-56710052361_20260906_141107.log
    ("bootloader_a만 체크" 시나리오 UART 캡처, 11.5절 근거 — 전체 이미지 로그와 시퀀스 동일함을 확인)

이미지 패킹 스크립트: /Volumes/jejezz/Work/AllWinnerT527/
  - android13/device/softwinner/common/vendorsetup.sh (_package() 함수, 297행)
  - android13/longan/build/pack (실제 패킹 로직 — boot_resource_list vs boot_file_list, 9.1절)
```

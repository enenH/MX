package moe.fuqiuluo.mamu.data.model

import moe.fuqiuluo.mamu.floating.model.DisplayValueType
import moe.fuqiuluo.mamu.floating.model.MemoryRange

data class SavedAddress(
    val address: Long, // 内存地址（也作为唯一标识符）
    val name: String, // 变量名称
    val valueType: Int, // 数据类型 ID
    val value: String, // 当前值
    val isFrozen: Boolean = false, // 是否冻结
    val range: MemoryRange,
    val timestamp: Long = System.currentTimeMillis() // 保存时间戳
) {
    val displayValueType: DisplayValueType?
        get() = DisplayValueType.fromNativeId(valueType)
}

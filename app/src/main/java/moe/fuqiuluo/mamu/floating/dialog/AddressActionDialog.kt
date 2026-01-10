package moe.fuqiuluo.mamu.floating.dialog

import android.annotation.SuppressLint
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.tencent.mmkv.MMKV
import kotlinx.coroutines.CoroutineScope
import moe.fuqiuluo.mamu.R
import moe.fuqiuluo.mamu.data.settings.getDialogOpacity
import moe.fuqiuluo.mamu.databinding.DialogAddressActionRvBinding
import moe.fuqiuluo.mamu.databinding.ItemAddressActionBinding
import moe.fuqiuluo.mamu.floating.data.model.DisplayValueType
import moe.fuqiuluo.mamu.floating.data.model.MemoryRange
import moe.fuqiuluo.mamu.floating.data.model.SavedAddress
import moe.fuqiuluo.mamu.utils.ValueTypeUtils
import moe.fuqiuluo.mamu.widget.NotificationOverlay
import moe.fuqiuluo.mamu.widget.RealtimeMonitorOverlay

/**
 * 地址操作对话框来源
 */
enum class AddressActionSource {
    /** 搜索结果界面 */
    SEARCH,
    /** 内存预览界面 */
    MEMORY_PREVIEW,
    /** 保存地址界面 */
    SAVED_ADDRESS
}

/**
 * 地址操作对话框 - 使用 RecyclerView 实现
 */
class AddressActionDialog(
    context: Context,
    private val notification: NotificationOverlay,
    private val clipboardManager: ClipboardManager,
    private val address: Long,
    private val value: String,
    private val valueType: DisplayValueType,
    private val coroutineScope: CoroutineScope,
    private val callbacks: Callbacks,
    private val source: AddressActionSource = AddressActionSource.SEARCH,
    private val memoryRange: MemoryRange? = null
) : BaseDialog(context) {

    /**
     * 回调接口
     */
    interface Callbacks {
        /**
         * 显示偏移量计算器（传入选中的地址）
         */
        fun onShowOffsetCalculator(address: Long)

        /**
         * 跳转到指定地址（在内存预览中）
         */
        fun onJumpToAddress(address: Long)
    }

    /**
     * 操作项数据类
     */
    private data class ActionItem(
        val title: String,
        val icon: Int,
        val action: () -> Unit
    )

    @SuppressLint("SetTextI18n")
    override fun setupDialog() {
        val binding = DialogAddressActionRvBinding.inflate(LayoutInflater.from(dialog.context))
        dialog.setContentView(binding.root)

        // 应用透明度设置
        val mmkv = MMKV.defaultMMKV()
        val opacity = mmkv.getDialogOpacity()
        binding.rootContainer.background?.alpha = (opacity * 255).toInt()

        // 显示地址信息
        binding.addressInfoText.text = "地址: 0x${address.toString(16).uppercase()}"
        binding.valueInfoText.text = "值: $value (${valueType.displayName})"

        // 定义所有操作列表
        val actions = buildActionList()

        // 设置 RecyclerView
        binding.actionRecyclerView.apply {
            layoutManager = LinearLayoutManager(context)
            adapter = ActionAdapter(actions)
            // 添加分隔线
            addItemDecoration(DividerItemDecoration())
        }

        // 取消按钮
        binding.btnCancel.setOnClickListener {
            dismiss()
        }
    }

    /**
     * 获取值的16进制字符串表示
     */
    private fun getHexString(): String {
        return try {
            val bytes = ValueTypeUtils.parseExprToBytes(value, valueType)
            bytes.joinToString("") { "%02X".format(it) }
        } catch (e: Exception) {
            // 回退到简单的整数转换
            "%X".format(value.toLongOrNull() ?: 0)
        }
    }

    /**
     * 获取值的反向16进制字符串表示（字节顺序反转）
     */
    private fun getReverseHexString(): String {
        return try {
            val bytes = ValueTypeUtils.parseExprToBytes(value, valueType)
            bytes.reversedArray().joinToString("") { "%02X".format(it) }
        } catch (e: Exception) {
            // 回退到简单的整数转换
            "%X".format(value.toLongOrNull() ?: 0).reversed()
        }
    }

    /**
     * 构建操作列表
     */
    private fun buildActionList(): List<ActionItem> {
        val hexString = getHexString()
        val reverseHexString = getReverseHexString()

        val actions = mutableListOf(
            ActionItem("偏移量计算器", R.drawable.calculate_24px) {
                dismiss()
                callbacks.onShowOffsetCalculator(address)
            },
            ActionItem(
                "转到此地址: ${"%X".format(address)}",
                R.drawable.icon_arrow_right_alt_24px
            ) {
                dismiss()
                callbacks.onJumpToAddress(address)
            },
            ActionItem(
                "跳转到指针: ${"%X".format(value.toLongOrNull() ?: 0)}",
                R.drawable.icon_arrow_right_alt_24px
            ) {
                dismiss()
                val pointerAddress = value.toLongOrNull() ?: return@ActionItem
                callbacks.onJumpToAddress(pointerAddress)
                notification.showSuccess("跳转到指针: 0x${pointerAddress.toString(16).uppercase()}")
            },
            ActionItem("复制此地址: ${"%X".format(address)}", R.drawable.content_copy_24px) {
                copyAddress()
            },
            ActionItem("复制此值: $value", R.drawable.content_copy_24px) {
                copyValue()
            },
            ActionItem(
                "复制16进制值: $hexString",
                R.drawable.content_copy_24px
            ) {
                copyHexValue()
            },
            ActionItem(
                "复制反16进制值: $reverseHexString",
                R.drawable.content_copy_24px
            ) {
                copyReverseHexValue()
            }
        )

        // 实时监视选项（所有来源都可用）
        actions.add(ActionItem("实时监视", R.drawable.icon_visibility_24px) {
            dismiss()
            showRealtimeMonitor()
        })

        return actions
    }

    /**
     * 显示实时监视悬浮窗（单个地址）
     */
    private fun showRealtimeMonitor() {
        val savedAddress = SavedAddress(
            address = address,
            name = "",
            valueType = valueType.nativeId,
            value = value,
            isFrozen = false,
            range = memoryRange ?: MemoryRange.An
        )
        RealtimeMonitorOverlay(dialog.context, listOf(savedAddress)).show()
        notification.showSuccess("已添加实时监视")
    }

    /**
     * 获取当前对话框来源
     */
    fun getSource(): AddressActionSource = source

    /**
     * 复制地址
     */
    private fun copyAddress() {
        val addressText = address.toString(16).uppercase()
        val clip = ClipData.newPlainText("address", addressText)
        clipboardManager.setPrimaryClip(clip)
        notification.showSuccess("已复制地址: $addressText")
        dismiss()
    }

    /**
     * 复制值
     */
    private fun copyValue() {
        val clip = ClipData.newPlainText("value", value)
        clipboardManager.setPrimaryClip(clip)
        notification.showSuccess("已复制值: $value")
        dismiss()
    }

    /**
     * 复制16进制值
     */
    private fun copyHexValue() {
        try {
            val bytes = ValueTypeUtils.parseExprToBytes(value, valueType)
            val hexString = bytes.joinToString("") { "%02X".format(it) }
            val clip = ClipData.newPlainText("hex_value", hexString)
            clipboardManager.setPrimaryClip(clip)
            notification.showSuccess("已复制16进制: $hexString")
            dismiss()
        } catch (e: Exception) {
            notification.showError("转换失败: ${e.message}")
        }
    }

    /**
     * 复制反16进制值（字节顺序反转）
     */
    private fun copyReverseHexValue() {
        try {
            val bytes = ValueTypeUtils.parseExprToBytes(value, valueType)
            val hexString = bytes.reversedArray().joinToString("") { "%02X".format(it) }
            val clip = ClipData.newPlainText("reverse_hex_value", hexString)
            clipboardManager.setPrimaryClip(clip)
            notification.showSuccess("已复制反16进制: $hexString")
            dismiss()
        } catch (e: Exception) {
            notification.showError("转换失败: ${e.message}")
        }
    }

    /**
     * RecyclerView 适配器
     */
    private inner class ActionAdapter(
        private val actions: List<ActionItem>
    ) : RecyclerView.Adapter<ActionAdapter.ViewHolder>() {

        inner class ViewHolder(
            private val binding: ItemAddressActionBinding
        ) : RecyclerView.ViewHolder(binding.root) {

            fun bind(item: ActionItem) {
                binding.actionTitle.text = item.title
                binding.actionIcon.setImageResource(item.icon)
                binding.itemContainer.setOnClickListener {
                    item.action()
                }
            }
        }

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): ViewHolder {
            val binding = ItemAddressActionBinding.inflate(
                LayoutInflater.from(parent.context),
                parent,
                false
            )
            return ViewHolder(binding)
        }

        override fun onBindViewHolder(holder: ViewHolder, position: Int) {
            holder.bind(actions[position])
        }

        override fun getItemCount(): Int = actions.size
    }

    /**
     * 分隔线装饰器
     */
    private class DividerItemDecoration : RecyclerView.ItemDecoration() {
        override fun getItemOffsets(
            outRect: android.graphics.Rect,
            view: android.view.View,
            parent: RecyclerView,
            state: RecyclerView.State
        ) {
            val position = parent.getChildAdapterPosition(view)
            if (position != parent.adapter?.itemCount?.minus(1)) {
                outRect.bottom = 1
            }
        }
    }
}
